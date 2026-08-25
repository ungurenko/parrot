use anyhow::{anyhow, Context, Result};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::Deserialize;
use std::env;
use std::ffi::OsString;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::AppHandle;

use crate::cancellation::{is_cancelled, CancelToken};
use crate::{mlx_env, paths};

pub const ENGINE_QWEN_0_6B: &str = "qwen-0.6b";
pub const ENGINE_QWEN_1_7B: &str = "qwen-1.7b";

const MODEL_QWEN_0_6B: &str = "Qwen/Qwen3-ASR-0.6B";
const MODEL_QWEN_1_7B: &str = "Qwen/Qwen3-ASR-1.7B";
const SERVER_API_KEY: &str = "parrot-local-qwen";
const READY_MARKER_PREFIX: &str = ".parrot-ready";
const RUNTIME_READY_MARKER: &str = ".parrot-ready-qwen-runtime-0.3.5-mlx-0.31.1";
const QWEN_RUNTIME_PACKAGES: &[&str] = &["mlx==0.31.1", "mlx-qwen3-asr[serve]==0.3.5"];
const QWEN_RUNTIME_CHECK: &str = r#"
from importlib.metadata import version
import fastapi
import uvicorn
import mlx
import mlx_qwen3_asr
assert version("mlx") == "0.31.1"
assert version("mlx-qwen3-asr") == "0.3.5"
"#;

/// Expected cache size per model (safetensors + tokenizer + config).
/// Used to detect incomplete downloads.
pub const EXPECTED_QWEN_0_6B_BYTES: u64 = 1_880_619_678;
pub const EXPECTED_QWEN_1_7B_BYTES: u64 = 4_703_114_308;

static SERVER: Lazy<Mutex<Option<QwenServer>>> = Lazy::new(|| Mutex::new(None));
static SERVER_CLIENT: Lazy<Result<reqwest::blocking::Client, reqwest::Error>> = Lazy::new(|| {
    reqwest::blocking::Client::builder()
        .timeout(mlx_env::SERVER_REQUEST_TIMEOUT)
        .build()
});

struct QwenServer {
    engine: String,
    port: u16,
    child: Child,
}

impl QwenServer {
    fn url(&self) -> String {
        format!("http://{}:{}", mlx_env::SERVER_HOST, self.port)
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }

    fn stop(&mut self) {
        let result = mlx_env::stop_child(&mut self.child);
        tracing::info!("Qwen MLX server stop: {result}");
    }
}

#[derive(Deserialize)]
struct TranscriptionResponse {
    text: String,
}

pub fn is_qwen_engine(engine: &str) -> bool {
    model_for_engine(engine).is_some()
}

pub fn model_for_engine(engine: &str) -> Option<&'static str> {
    match engine {
        ENGINE_QWEN_0_6B => Some(MODEL_QWEN_0_6B),
        ENGINE_QWEN_1_7B => Some(MODEL_QWEN_1_7B),
        _ => None,
    }
}

pub fn expected_bytes_for_engine(engine: &str) -> Option<u64> {
    match engine {
        ENGINE_QWEN_0_6B => Some(EXPECTED_QWEN_0_6B_BYTES),
        ENGINE_QWEN_1_7B => Some(EXPECTED_QWEN_1_7B_BYTES),
        _ => None,
    }
}

pub fn is_ready(app: &AppHandle, engine: &str) -> bool {
    runtime_ready_marker_exists(app) && resolve_cli(app).is_ok() && model_cache_ready(app, engine)
}

pub fn unavailable_reason() -> Option<String> {
    let product_version = detected_macos_version();
    qwen_platform_unavailable_reason(std::env::consts::ARCH, product_version.as_deref())
}

pub fn install_runtime<F: Fn(&str) + Send + Sync>(app: &AppHandle, on_progress: F) -> Result<()> {
    if let Some(reason) = unavailable_reason() {
        anyhow::bail!(reason);
    }
    mlx_env::with_install_lock(|| install_runtime_locked(app, &on_progress))
}

fn install_runtime_locked<F: Fn(&str) + Send + Sync>(
    app: &AppHandle,
    on_progress: &F,
) -> Result<()> {
    let venv_python = mlx_env::ensure_user_python_venv(app, on_progress)?;
    let cli = paths::qwen_env_dir(app)?.join("bin/mlx-qwen3-asr");

    if qwen_runtime_ready_at(&venv_python, &cli) {
        write_runtime_ready_marker(app)?;
        on_progress("Окружение Qwen уже готово");
        return Ok(());
    }

    remove_runtime_ready_marker(app);
    on_progress("Устанавливаю Qwen MLX…");
    let mut pip_args = vec![
        "--disable-pip-version-check",
        "--upgrade",
        "--force-reinstall",
    ];
    pip_args.extend_from_slice(QWEN_RUNTIME_PACKAGES);
    let output = mlx_env::pip_install(
        &venv_python,
        &pip_args,
        "Не удалось запустить pip install для Qwen MLX",
    )?;
    if !output.status.success() {
        return Err(mlx_env::command_error(
            "Не удалось установить Qwen MLX",
            &output.stderr,
            output.status.code(),
        ));
    }
    if !qwen_runtime_ready_at(&venv_python, &cli) {
        return Err(anyhow!(
            "Qwen MLX установлен, но проверка окружения не прошла. Попробуйте повторить установку."
        ));
    }
    write_runtime_ready_marker(app)?;
    on_progress("Окружение Qwen готово");
    Ok(())
}

fn runtime_ready_marker_path(app: &AppHandle) -> Result<PathBuf> {
    let env_dir = paths::qwen_env_dir(app)?;
    let root = env_dir
        .parent()
        .ok_or_else(|| anyhow!("Некорректный путь окружения Qwen"))?;
    Ok(root.join(RUNTIME_READY_MARKER))
}

fn runtime_ready_marker_exists(app: &AppHandle) -> bool {
    runtime_ready_marker_path(app)
        .map(|path| path.is_file())
        .unwrap_or(false)
}

fn write_runtime_ready_marker(app: &AppHandle) -> Result<()> {
    let path = runtime_ready_marker_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, b"ok")?;
    Ok(())
}

fn remove_runtime_ready_marker(app: &AppHandle) {
    if let Ok(path) = runtime_ready_marker_path(app) {
        let _ = std::fs::remove_file(path);
    }
}

fn qwen_runtime_ready_at(python: &Path, cli: &Path) -> bool {
    if !python.is_file() || !cli.is_file() {
        return false;
    }
    let imports_ready = Command::new(python)
        .args(["-c", QWEN_RUNTIME_CHECK])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    imports_ready
        && Command::new(cli)
            .arg("--help")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
}

pub fn model_cache_dir(app: &AppHandle, engine: &str) -> Result<PathBuf> {
    let model =
        model_for_engine(engine).ok_or_else(|| anyhow!("unknown Qwen MLX engine: {engine}"))?;
    Ok(paths::qwen_cache_dir(app)?
        .join("hub")
        .join(mlx_env::repo_cache_name(model)))
}

fn qwen_platform_unavailable_reason(
    target_arch: &str,
    product_version: Option<&str>,
) -> Option<String> {
    if target_arch != "aarch64" {
        return Some("Qwen MLX доступен только на Mac с Apple Silicon.".to_string());
    }

    let Some(major) = product_version.and_then(parse_macos_major) else {
        return Some(
            "Не удалось определить версию macOS. Qwen MLX требует macOS 14 или новее.".to_string(),
        );
    };
    if major < 14 {
        return Some(
            "Qwen MLX требует macOS 14 или новее. На этом Mac используйте Parakeet или Whisper."
                .to_string(),
        );
    }
    None
}

fn parse_macos_major(product_version: &str) -> Option<u32> {
    product_version.trim().split('.').next()?.parse().ok()
}

#[cfg(target_os = "macos")]
fn detected_macos_version() -> Option<String> {
    let output = Command::new("/usr/bin/sw_vers")
        .arg("-productVersion")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let version = String::from_utf8(output.stdout).ok()?;
    let version = version.trim();
    (!version.is_empty()).then(|| version.to_string())
}

#[cfg(not(target_os = "macos"))]
fn detected_macos_version() -> Option<String> {
    None
}

pub fn warmup_model(app: &AppHandle, engine: &str, cancel: Arc<CancelToken>) -> Result<()> {
    let model =
        model_for_engine(engine).ok_or_else(|| anyhow!("unknown Qwen MLX engine: {engine}"))?;
    let cli = resolve_cli(app)?;
    let cache_dir = paths::qwen_cache_dir(app)?;
    let tmp = paths::tmp_dir(app)?;
    let warmup_wav = tmp.join(format!("qwen-warmup-{}.wav", engine.replace('.', "-")));

    crate::transcriber::write_silent_wav(&warmup_wav)?;
    if cancel.is_cancelled() {
        anyhow::bail!("cancelled");
    }
    let child = qwen_command(&cli, &cache_dir)
        .arg(&warmup_wav)
        .arg("--model")
        .arg(model)
        .arg("--stdout-only")
        .arg("--no-progress")
        .spawn();
    let child = match child {
        Ok(child) => child,
        Err(e) => {
            let _ = std::fs::remove_file(&warmup_wav);
            return Err(e.into());
        }
    };
    let pid = child.id();
    cancel.register_pid(pid);
    let output = child.wait_with_output();
    cancel.unregister_pid(pid);
    let _ = std::fs::remove_file(&warmup_wav);

    let output = output?;
    if !output.status.success() {
        if cancel.is_cancelled() {
            anyhow::bail!("cancelled");
        }
        return Err(mlx_env::command_error(
            "Qwen MLX не смог подготовить модель",
            &output.stderr,
            output.status.code(),
        ));
    }
    write_ready_marker(&cache_dir, engine)?;
    Ok(())
}

pub fn preload_server(app: &AppHandle, engine: &str) -> Result<()> {
    ensure_server(app, engine).map(|_| ())
}

pub fn stop_server() {
    if let Some(mut server) = SERVER.lock().take() {
        server.stop();
    }
}

pub fn clear_ready_marker(app: &AppHandle, engine: &str) {
    if let Ok(cache_dir) = paths::qwen_cache_dir(app) {
        remove_ready_marker(&cache_dir, engine);
    }
}

pub fn transcribe_wav(
    app: &AppHandle,
    wav_path: &Path,
    engine: &str,
    language: &str,
    cancel: Option<Arc<CancelToken>>,
    progress_cb: impl Fn(u32) + Send + Sync,
) -> Result<String> {
    let model =
        model_for_engine(engine).ok_or_else(|| anyhow!("unknown Qwen MLX engine: {engine}"))?;

    progress_cb(5);
    let server_attempt_started = Instant::now();
    match transcribe_with_server(app, wav_path, engine, model, language, cancel.clone()) {
        Ok(text) => {
            tracing::info!(
                "Qwen warm server transcription finished in {:.2}s",
                server_attempt_started.elapsed().as_secs_f64()
            );
            progress_cb(100);
            return Ok(text);
        }
        Err(e) => {
            if is_cancelled(&cancel) {
                return Err(anyhow!("cancelled"));
            }
            tracing::warn!(
                "Qwen warm server failed after {:.2}s, falling back to per-job CLI: {e:#}",
                server_attempt_started.elapsed().as_secs_f64()
            );
        }
    }

    let cli = resolve_cli(app)?;
    let cache_dir = paths::qwen_cache_dir(app)?;

    let mut command = qwen_command(&cli, &cache_dir);
    command
        .arg(wav_path)
        .arg("--model")
        .arg(model)
        .arg("--stdout-only")
        .arg("--no-progress");
    if let Some(qwen_language) = qwen_language(language) {
        command.arg("--language").arg(qwen_language);
    }
    let child = command.spawn()?;

    let pid = child.id();
    if let Some(tok) = cancel.as_ref() {
        tok.register_pid(pid);
    }
    let fallback_started = Instant::now();
    let output = child.wait_with_output()?;
    if let Some(tok) = cancel.as_ref() {
        tok.unregister_pid(pid);
    }

    if !output.status.success() {
        if is_cancelled(&cancel) {
            return Err(anyhow!("cancelled"));
        }
        return Err(mlx_env::command_error(
            "Qwen MLX не смог расшифровать аудио",
            &output.stderr,
            output.status.code(),
        ));
    }

    progress_cb(100);
    tracing::info!(
        "Qwen per-job CLI fallback finished in {:.2}s",
        fallback_started.elapsed().as_secs_f64()
    );
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn transcribe_with_server(
    app: &AppHandle,
    wav_path: &Path,
    engine: &str,
    model: &str,
    language: &str,
    cancel: Option<Arc<CancelToken>>,
) -> Result<String> {
    let (url, pid) = {
        let mut guard = SERVER.lock();
        ensure_server_locked(app, engine, &mut guard)?
    };

    if let Some(tok) = cancel.as_ref() {
        tok.register_pid(pid);
    }

    let result = post_to_server(&url, wav_path, model, language);

    if let Some(tok) = cancel.as_ref() {
        tok.unregister_pid(pid);
    }

    result
}

fn ensure_server(app: &AppHandle, engine: &str) -> Result<()> {
    let mut guard = SERVER.lock();
    ensure_server_locked(app, engine, &mut guard)?;
    Ok(())
}

fn ensure_server_locked(
    app: &AppHandle,
    engine: &str,
    guard: &mut Option<QwenServer>,
) -> Result<(String, u32)> {
    if let Some(server) = guard.as_mut() {
        if server.engine == engine
            && server.child.try_wait()?.is_none()
            && mlx_env::health_ok(server.port)
        {
            return Ok((server.url(), server.pid()));
        }
    }

    if let Some(mut old) = guard.take() {
        old.stop();
    }

    let model =
        model_for_engine(engine).ok_or_else(|| anyhow!("unknown Qwen MLX engine: {engine}"))?;
    let cli = resolve_cli(app)?;
    let cache_dir = paths::qwen_cache_dir(app)?;
    let port = find_free_port()?;

    let server_start = Instant::now();
    let mut child = qwen_server_command(&cli, &cache_dir)
        .arg("serve")
        .arg("--host")
        .arg(mlx_env::SERVER_HOST)
        .arg("--port")
        .arg(port.to_string())
        .arg("--api-key")
        .arg(SERVER_API_KEY)
        .arg("--model")
        .arg(model)
        .spawn()
        .context("failed to start Qwen MLX server")?;

    wait_for_server(&mut child, port)?;
    tracing::info!(
        "Qwen MLX warm server started on port {port} in {:.2}s",
        server_start.elapsed().as_secs_f64()
    );

    *guard = Some(QwenServer {
        engine: engine.to_string(),
        port,
        child,
    });
    let server = guard.as_ref().expect("server just inserted");
    Ok((server.url(), server.pid()))
}

fn post_to_server(url: &str, wav_path: &Path, model: &str, language: &str) -> Result<String> {
    let file = File::open(wav_path)?;
    let file_name = wav_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "audio.wav".to_string());
    let part = reqwest::blocking::multipart::Part::reader(file)
        .file_name(file_name)
        .mime_str("audio/wav")?;
    let mut form = reqwest::blocking::multipart::Form::new()
        .part("file", part)
        .text("model", model.to_string())
        .text("response_format", "json");
    if let Some(qwen_language) = qwen_language(language) {
        form = form.text("language", qwen_language.to_string());
    }

    let client = server_client()?;
    let request_started = Instant::now();
    let response = client
        .post(format!("{url}/v1/audio/transcriptions"))
        .bearer_auth(SERVER_API_KEY)
        .multipart(form)
        .send()?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(anyhow!("Qwen server returned {status}: {body}"));
    }

    let text = response
        .json::<TranscriptionResponse>()?
        .text
        .trim()
        .to_string();
    tracing::info!(
        "Qwen warm server request finished in {:.2}s",
        request_started.elapsed().as_secs_f64()
    );
    Ok(text)
}

fn qwen_language(language: &str) -> Option<&'static str> {
    match language {
        "ru" => Some("Russian"),
        "en" => Some("English"),
        "de" => Some("German"),
        "fr" => Some("French"),
        "es" => Some("Spanish"),
        "it" => Some("Italian"),
        "pt" => Some("Portuguese"),
        "uk" => Some("Ukrainian"),
        _ => None,
    }
}

fn wait_for_server(child: &mut Child, port: u16) -> Result<()> {
    let started = Instant::now();
    while started.elapsed() < mlx_env::SERVER_START_TIMEOUT {
        if let Some(status) = child.try_wait()? {
            return Err(anyhow!("Qwen MLX server exited during startup: {status}"));
        }
        if mlx_env::health_ok(port) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let _ = child.kill();
    let _ = child.wait();
    Err(anyhow!("Qwen MLX server startup timed out"))
}

fn server_client() -> Result<&'static reqwest::blocking::Client> {
    SERVER_CLIENT
        .as_ref()
        .map_err(|e| anyhow!("failed to create Qwen server HTTP client: {e}"))
}

fn find_free_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind((mlx_env::SERVER_HOST, 0))?;
    Ok(listener.local_addr()?.port())
}

fn model_cache_ready(app: &AppHandle, engine: &str) -> bool {
    let Some(model) = model_for_engine(engine) else {
        return false;
    };
    let Some(expected) = expected_bytes_for_engine(engine) else {
        return false;
    };
    let Ok(cache_dir) = paths::qwen_cache_dir(app) else {
        return false;
    };
    mlx_env::model_cache_ready(app, model, expected) && ready_marker_exists(&cache_dir, engine)
}

fn write_ready_marker(cache_dir: &Path, engine: &str) -> Result<()> {
    let path = ready_marker_path(cache_dir, engine)?;
    std::fs::write(path, b"ok")?;
    Ok(())
}

fn remove_ready_marker(cache_dir: &Path, engine: &str) {
    if let Ok(path) = ready_marker_path(cache_dir, engine) {
        let _ = std::fs::remove_file(path);
    }
}

fn ready_marker_exists(cache_dir: &Path, engine: &str) -> bool {
    ready_marker_path(cache_dir, engine)
        .map(|path| path.is_file())
        .unwrap_or(false)
}

fn ready_marker_path(cache_dir: &Path, engine: &str) -> Result<PathBuf> {
    if !is_qwen_engine(engine) {
        anyhow::bail!("unknown Qwen MLX engine: {engine}");
    }
    Ok(cache_dir.join(format!(
        "{READY_MARKER_PREFIX}-{}",
        engine.replace('.', "-")
    )))
}

fn resolve_cli(app: &AppHandle) -> Result<PathBuf> {
    let explicit = env::var_os("PARROT_QWEN_BIN")
        .or_else(|| env::var_os("AUDIO_TO_TEXT_QWEN_BIN"))
        .map(PathBuf::from);
    let app_venv_cli = paths::qwen_env_dir(app)
        .ok()
        .map(|dir| dir.join("bin/mlx-qwen3-asr"));
    resolve_cli_from_candidates(
        explicit,
        app_venv_cli,
        &mlx_env::candidate_roots(),
        env::var_os("PATH"),
    )
}

fn resolve_cli_from_candidates(
    explicit: Option<PathBuf>,
    app_venv_cli: Option<PathBuf>,
    roots: &[PathBuf],
    path_var: Option<OsString>,
) -> Result<PathBuf> {
    if let Some(path) = explicit {
        if path.is_file() {
            return Ok(path);
        }
        return Err(anyhow!(
            "Qwen MLX не найден по указанному пути: {}",
            path.display()
        ));
    }

    if let Some(path) = app_venv_cli.filter(|path| path.is_file()) {
        return Ok(path);
    }

    for root in roots {
        let path = root.join(".qwen-mlx/venv/bin/mlx-qwen3-asr");
        if path.is_file() {
            return Ok(path);
        }
    }

    if let Some(path) = find_in_path_var("mlx-qwen3-asr", path_var) {
        return Ok(path);
    }

    Err(anyhow!(
        "Qwen MLX ещё не установлен. Откройте настройки и нажмите «Скачать и выбрать»."
    ))
}

fn find_in_path_var(name: &str, path_var: Option<OsString>) -> Option<PathBuf> {
    let path_var = path_var?;
    env::split_paths(&path_var)
        .map(|part| part.join(name))
        .find(|path| path.is_file())
}

fn qwen_command(cli: &Path, cache_dir: &Path) -> Command {
    mlx_env::python_command(cli, cache_dir)
}

fn qwen_server_command(cli: &Path, cache_dir: &Path) -> Command {
    let mut command = qwen_command(cli, cache_dir);
    command.stdout(Stdio::null()).stderr(Stdio::null());
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "parrot-qwen-ready-test-{}-{}",
            name,
            std::process::id()
        ))
    }

    #[test]
    fn ready_marker_is_engine_scoped() {
        let dir = temp_dir("marker");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");

        assert!(!ready_marker_exists(&dir, ENGINE_QWEN_0_6B));
        write_ready_marker(&dir, ENGINE_QWEN_0_6B).expect("write marker");

        assert!(ready_marker_exists(&dir, ENGINE_QWEN_0_6B));
        assert!(!ready_marker_exists(&dir, ENGINE_QWEN_1_7B));

        remove_ready_marker(&dir, ENGINE_QWEN_0_6B);
        assert!(!ready_marker_exists(&dir, ENGINE_QWEN_0_6B));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn expected_size_is_engine_scoped() {
        assert_eq!(
            expected_bytes_for_engine(ENGINE_QWEN_0_6B),
            Some(EXPECTED_QWEN_0_6B_BYTES)
        );
        assert_eq!(
            expected_bytes_for_engine(ENGINE_QWEN_1_7B),
            Some(EXPECTED_QWEN_1_7B_BYTES)
        );
        assert_eq!(expected_bytes_for_engine("unknown"), None);
    }

    #[test]
    fn qwen_platform_accepts_apple_silicon_on_macos_14_or_newer() {
        assert_eq!(
            qwen_platform_unavailable_reason("aarch64", Some("14.0")),
            None
        );
        assert_eq!(
            qwen_platform_unavailable_reason("aarch64", Some("15.6.1")),
            None
        );
    }

    #[test]
    fn qwen_platform_rejects_old_unknown_and_non_apple_silicon_systems() {
        assert!(qwen_platform_unavailable_reason("aarch64", Some("13.7"))
            .expect("old macOS reason")
            .contains("macOS 14"));
        assert!(qwen_platform_unavailable_reason("aarch64", None)
            .expect("unknown macOS reason")
            .contains("Не удалось определить"));
        assert!(qwen_platform_unavailable_reason("x86_64", Some("15.0"))
            .expect("Intel reason")
            .contains("Apple Silicon"));
    }

    #[test]
    fn macos_major_parser_is_conservative() {
        assert_eq!(parse_macos_major("14.6.1"), Some(14));
        assert_eq!(parse_macos_major("15"), Some(15));
        assert_eq!(parse_macos_major(""), None);
        assert_eq!(parse_macos_major("unknown"), None);
    }

    #[test]
    fn cli_resolution_finds_application_support_venv() {
        let dir = temp_dir("app-venv-cli");
        let cli = dir.join("bin/mlx-qwen3-asr");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(cli.parent().expect("cli parent")).expect("create cli dir");
        std::fs::write(&cli, b"#!/bin/sh\n").expect("write fake cli");

        let resolved = resolve_cli_from_candidates(None, Some(cli.clone()), &[], None)
            .expect("resolve app venv CLI");

        assert_eq!(resolved, cli);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn qwen_install_keeps_other_shared_venv_runtimes() {
        assert_eq!(
            QWEN_RUNTIME_PACKAGES,
            &["mlx==0.31.1", "mlx-qwen3-asr[serve]==0.3.5"]
        );
        assert!(!QWEN_RUNTIME_PACKAGES
            .iter()
            .any(|package| package.starts_with("mlx-lm")
                || package.starts_with("mlx-vlm")
                || package.starts_with("parakeet-mlx")));
    }

    #[cfg(unix)]
    #[test]
    fn runtime_readiness_handles_clean_and_already_prepared_venvs() {
        let dir = temp_dir("runtime-ready");
        let python = dir.join("bin/python");
        let cli = dir.join("bin/mlx-qwen3-asr");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(python.parent().expect("runtime parent"))
            .expect("create runtime dir");

        assert!(!qwen_runtime_ready_at(&python, &cli));

        std::fs::write(&python, b"#!/bin/sh\nexit 0\n").expect("write fake python");
        std::fs::write(&cli, b"#!/bin/sh\nexit 0\n").expect("write fake cli");
        let mut permissions = std::fs::metadata(&python)
            .expect("python metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&python, permissions).expect("make python executable");
        let mut cli_permissions = std::fs::metadata(&cli).expect("cli metadata").permissions();
        cli_permissions.set_mode(0o755);
        std::fs::set_permissions(&cli, cli_permissions).expect("make cli executable");

        assert!(qwen_runtime_ready_at(&python, &cli));
        assert!(qwen_runtime_ready_at(&python, &cli));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
