use anyhow::{anyhow, Context, Result};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::Deserialize;
use std::env;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::AppHandle;

use crate::cancellation::CancelToken;
use crate::paths;

pub const ENGINE_QWEN_0_6B: &str = "qwen-0.6b";
pub const ENGINE_QWEN_1_7B: &str = "qwen-1.7b";

const MODEL_QWEN_0_6B: &str = "Qwen/Qwen3-ASR-0.6B";
const MODEL_QWEN_1_7B: &str = "Qwen/Qwen3-ASR-1.7B";
const SERVER_HOST: &str = "127.0.0.1";
const SERVER_API_KEY: &str = "parrot-local-qwen";
const SERVER_START_TIMEOUT: Duration = Duration::from_secs(90);
const SERVER_REQUEST_TIMEOUT: Duration = Duration::from_secs(60 * 60);

/// Expected cache size per model (safetensors + tokenizer + config).
/// Used to detect incomplete downloads.
pub const EXPECTED_QWEN_0_6B_BYTES: u64 = 1_100_000_000;
pub const EXPECTED_QWEN_1_7B_BYTES: u64 = 3_300_000_000;

static SERVER: Lazy<Mutex<Option<QwenServer>>> = Lazy::new(|| Mutex::new(None));

struct QwenServer {
    engine: String,
    port: u16,
    child: Child,
}

impl QwenServer {
    fn url(&self) -> String {
        format!("http://{SERVER_HOST}:{}", self.port)
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }

    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
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

pub fn is_ready(app: &AppHandle, engine: &str) -> bool {
    resolve_cli().is_ok() && model_cache_exists(app, engine)
}

pub fn unavailable_reason() -> Option<String> {
    if resolve_cli().is_ok() {
        None
    } else {
        Some(
            "Qwen MLX пока не установлен на этом Mac. Для стабильной версии используйте Parakeet или Whisper."
                .to_string(),
        )
    }
}

pub fn warmup_model(app: &AppHandle, engine: &str) -> Result<()> {
    let model =
        model_for_engine(engine).ok_or_else(|| anyhow!("unknown Qwen MLX engine: {engine}"))?;
    let cli = resolve_cli()?;
    let cache_dir = paths::qwen_cache_dir(app)?;
    let tmp = paths::tmp_dir(app)?;
    let warmup_wav = tmp.join(format!("qwen-warmup-{}.wav", engine.replace('.', "-")));

    write_silent_wav(&warmup_wav)?;
    let output = qwen_command(&cli, &cache_dir)
        .arg(&warmup_wav)
        .arg("--model")
        .arg(model)
        .arg("--stdout-only")
        .arg("--no-progress")
        .output();
    let _ = std::fs::remove_file(&warmup_wav);

    let output = output?;
    if !output.status.success() {
        return Err(command_error(
            "Qwen MLX не смог подготовить модель",
            &output.stderr,
            output.status.code(),
        ));
    }
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
    match transcribe_with_server(app, wav_path, engine, model, language, cancel.clone()) {
        Ok(text) => {
            progress_cb(100);
            return Ok(text);
        }
        Err(e) => {
            if cancel.as_ref().map(|t| t.is_cancelled()).unwrap_or(false) {
                return Err(anyhow!("cancelled"));
            }
            tracing::warn!("Qwen warm server failed, falling back to per-job CLI: {e:#}");
        }
    }

    let cli = resolve_cli()?;
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
    let output = child.wait_with_output()?;
    if let Some(tok) = cancel.as_ref() {
        tok.unregister_pid(pid);
    }

    if !output.status.success() {
        if cancel.as_ref().map(|t| t.is_cancelled()).unwrap_or(false) {
            return Err(anyhow!("cancelled"));
        }
        return Err(command_error(
            "Qwen MLX не смог расшифровать аудио",
            &output.stderr,
            output.status.code(),
        ));
    }

    progress_cb(100);
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
        if server.engine == engine && server.child.try_wait()?.is_none() && health_ok(server.port) {
            return Ok((server.url(), server.pid()));
        }
    }

    if let Some(mut old) = guard.take() {
        old.stop();
    }

    let model =
        model_for_engine(engine).ok_or_else(|| anyhow!("unknown Qwen MLX engine: {engine}"))?;
    let cli = resolve_cli()?;
    let cache_dir = paths::qwen_cache_dir(app)?;
    let port = find_free_port()?;

    let mut child = qwen_server_command(&cli, &cache_dir)
        .arg("serve")
        .arg("--host")
        .arg(SERVER_HOST)
        .arg("--port")
        .arg(port.to_string())
        .arg("--api-key")
        .arg(SERVER_API_KEY)
        .arg("--model")
        .arg(model)
        .spawn()
        .context("failed to start Qwen MLX server")?;

    wait_for_server(&mut child, port)?;

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

    let client = reqwest::blocking::Client::builder()
        .timeout(SERVER_REQUEST_TIMEOUT)
        .build()?;
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

    Ok(response
        .json::<TranscriptionResponse>()?
        .text
        .trim()
        .to_string())
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
    while started.elapsed() < SERVER_START_TIMEOUT {
        if let Some(status) = child.try_wait()? {
            return Err(anyhow!("Qwen MLX server exited during startup: {status}"));
        }
        if health_ok(port) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let _ = child.kill();
    let _ = child.wait();
    Err(anyhow!("Qwen MLX server startup timed out"))
}

fn health_ok(port: u16) -> bool {
    let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(700))
        .build()
    else {
        return false;
    };
    client
        .get(format!("http://{SERVER_HOST}:{port}/health"))
        .send()
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

fn find_free_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind((SERVER_HOST, 0))?;
    Ok(listener.local_addr()?.port())
}

fn model_cache_exists(app: &AppHandle, engine: &str) -> bool {
    let Some(model) = model_for_engine(engine) else {
        return false;
    };
    let Ok(cache_dir) = paths::qwen_cache_dir(app) else {
        return false;
    };
    let repo_cache_name = format!("models--{}", model.replace('/', "--"));
    let model_dir = cache_dir.join("hub").join(repo_cache_name);
    if !model_dir.exists() {
        return false;
    }
    // Guard against partial downloads: require at least 90% of expected size.
    let expected = match engine {
        ENGINE_QWEN_0_6B => EXPECTED_QWEN_0_6B_BYTES,
        ENGINE_QWEN_1_7B => EXPECTED_QWEN_1_7B_BYTES,
        _ => return false,
    };
    dir_size(&model_dir) >= (expected as f64 * 0.9) as u64
}

fn dir_size(path: &Path) -> u64 {
    let mut total: u64 = 0;
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() {
                total = total.saturating_add(meta.len());
            } else if meta.is_dir() {
                total = total.saturating_add(dir_size(&p));
            }
        }
    }
    total
}

fn resolve_cli() -> Result<PathBuf> {
    if let Some(path) =
        env::var_os("PARROT_QWEN_BIN").or_else(|| env::var_os("AUDIO_TO_TEXT_QWEN_BIN"))
    {
        let path = PathBuf::from(path);
        if path.exists() {
            return Ok(path);
        }
        return Err(anyhow!(
            "Qwen MLX не найден по указанному пути: {}",
            path.display()
        ));
    }

    for root in candidate_roots() {
        let path = root.join(".qwen-mlx/venv/bin/mlx-qwen3-asr");
        if path.exists() {
            return Ok(path);
        }
    }

    if let Some(path) = find_in_path("mlx-qwen3-asr") {
        return Ok(path);
    }

    Err(anyhow!(
        "Qwen MLX не установлен. Запустите tools/setup_qwen_mlx.sh или укажите PARROT_QWEN_BIN."
    ))
}

fn candidate_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Ok(current) = env::current_dir() {
        roots.push(current.clone());
        if let Some(parent) = current.parent() {
            roots.push(parent.to_path_buf());
        }
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    roots.push(manifest_dir.clone());
    if let Some(parent) = manifest_dir.parent() {
        roots.push(parent.to_path_buf());
    }

    roots
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    env::split_paths(&path_var)
        .map(|part| part.join(name))
        .find(|path| path.exists())
}

fn qwen_command(cli: &Path, cache_dir: &Path) -> Command {
    let mut command = Command::new(cli);
    command
        .env("HF_HOME", cache_dir)
        .env("HF_HUB_DISABLE_TELEMETRY", "1")
        .env("HF_HUB_DISABLE_XET", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn qwen_server_command(cli: &Path, cache_dir: &Path) -> Command {
    let mut command = qwen_command(cli, cache_dir);
    command.stdout(Stdio::null()).stderr(Stdio::null());
    command
}

fn command_error(prefix: &str, stderr: &[u8], code: Option<i32>) -> anyhow::Error {
    let tail = String::from_utf8_lossy(stderr)
        .lines()
        .rev()
        .take(20)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    if tail.trim().is_empty() {
        anyhow!("{prefix}. Код завершения: {code:?}")
    } else {
        anyhow!("{prefix}: {}", tail.trim())
    }
}

fn write_silent_wav(path: &Path) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for _ in 0..16_000 {
        writer.write_sample::<i16>(0)?;
    }
    writer.finalize()?;
    Ok(())
}
