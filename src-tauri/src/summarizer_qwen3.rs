use anyhow::{anyhow, Context, Result};
use flate2::read::GzDecoder;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tar::Archive;
use tauri::AppHandle;

use crate::cancellation::CancelToken;
use crate::fs_metrics::dir_size_bytes;
use crate::prompts;
use crate::{paths, settings, summarizer_models};
use summarizer_models::{SummaryModelSpec, SummaryRuntime};

// Pinned standalone Python release from astral-sh/python-build-standalone.
// To update: pick newer tag at https://github.com/astral-sh/python-build-standalone/releases,
// fetch matching SHA256 from SHA256SUMS in that release, replace both constants.
// Tarball expands to a top-level `python/` directory (~17.8 MB compressed, ~60 MB unpacked).
const STANDALONE_PYTHON_URL: &str = "https://github.com/astral-sh/python-build-standalone/releases/download/20260414/cpython-3.12.13+20260414-aarch64-apple-darwin-install_only.tar.gz";
const STANDALONE_PYTHON_SHA256: &str =
    "8966b2bcd9fa03ba22c080ad15a86bc12e41a00122b16f4b3740e302261124d9";
const STANDALONE_PYTHON_BYTES: u64 = 17_836_558;
const DOWNLOAD_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const DOWNLOAD_TOTAL_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const SERVER_HOST: &str = "127.0.0.1";
const SERVER_START_TIMEOUT: Duration = Duration::from_secs(90);
const SERVER_REQUEST_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const HEALTH_TIMEOUT: Duration = Duration::from_millis(700);

const LEGACY_QWEN_READY_MARKER: &str = ".parrot-ready-summary";

const SUMMARY_MAX_TOKENS: u32 = 4096;
const SUMMARY_TEMP: f32 = 0.3;
const SUMMARY_TOP_P: f32 = 0.9;

static SUMMARY_SERVER: Lazy<Mutex<Option<SummaryServer>>> = Lazy::new(|| Mutex::new(None));
static SERVER_CLIENT: Lazy<Result<reqwest::blocking::Client, reqwest::Error>> = Lazy::new(|| {
    reqwest::blocking::Client::builder()
        .timeout(SERVER_REQUEST_TIMEOUT)
        .build()
});
static HEALTH_CLIENT: Lazy<Result<reqwest::blocking::Client, reqwest::Error>> = Lazy::new(|| {
    reqwest::blocking::Client::builder()
        .timeout(HEALTH_TIMEOUT)
        .build()
});

struct SummaryServer {
    model_id: &'static str,
    runtime: SummaryRuntime,
    port: u16,
    child: Child,
}

impl SummaryServer {
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SummarizerStatus {
    pub available: bool,
    pub model_ready: bool,
    pub unavailable_reason: Option<String>,
}

pub fn status(app: &AppHandle) -> SummarizerStatus {
    let unavailable_reason = match resolve_python(app) {
        Ok(_) => None,
        Err(e) => Some(e.to_string()),
    };
    let model_ready = unavailable_reason.is_none() && model_cache_exists(app);
    SummarizerStatus {
        available: unavailable_reason.is_none(),
        model_ready,
        unavailable_reason,
    }
}

pub fn is_ready(app: &AppHandle) -> bool {
    resolve_python(app).is_ok() && model_cache_exists(app)
}

pub fn expected_summary_bytes(app: &AppHandle) -> u64 {
    selected_model(app).expected_bytes
}

pub fn selected_model_label(app: &AppHandle) -> &'static str {
    selected_model(app).label
}

fn selected_model(app: &AppHandle) -> &'static SummaryModelSpec {
    let settings = settings::load(app);
    summarizer_models::summary_model_spec(&settings.summary_model)
        .unwrap_or(&summarizer_models::QWEN3_4B_SUMMARY)
}

pub fn model_cache_exists(app: &AppHandle) -> bool {
    let Ok(cache_dir) = paths::qwen_cache_dir(app) else {
        return false;
    };
    let spec = selected_model(app);
    if !ready_marker_exists_for(&cache_dir, spec) {
        return false;
    }
    model_cache_exists_in(&cache_dir, spec)
}

fn model_cache_exists_in(cache_dir: &Path, spec: &SummaryModelSpec) -> bool {
    let repo_cache_name = format!("models--{}", spec.repo.replace('/', "--"));
    let model_dir = cache_dir.join("hub").join(repo_cache_name);
    if !model_dir.exists() {
        return false;
    }
    dir_size_bytes(&model_dir) >= (spec.expected_bytes as f64 * 0.9) as u64
}

pub fn delete_model(app: &AppHandle) -> Result<()> {
    stop_server();
    let cache_dir = paths::qwen_cache_dir(app)?;
    let spec = selected_model(app);
    remove_ready_marker_for(&cache_dir, spec);
    let repo_cache_name = format!("models--{}", spec.repo.replace('/', "--"));
    let model_dir = cache_dir.join("hub").join(repo_cache_name);
    if model_dir.is_dir() {
        std::fs::remove_dir_all(&model_dir)?;
    }
    Ok(())
}

/// Download the model weights by running a short generation that triggers HF download.
pub fn warmup_model(app: &AppHandle, cancel: Arc<CancelToken>) -> Result<()> {
    if cancel.is_cancelled() {
        anyhow::bail!("cancelled");
    }
    let started = Instant::now();
    let spec = selected_model(app);
    let model_label = selected_model_label(app);
    run_summary_generate(
        app,
        spec,
        MlxLmGenerateRequest {
            prompt_arg: Some("Привет"),
            max_tokens: 4,
            ..MlxLmGenerateRequest::default()
        },
        cancel,
        "Не удалось подготовить модель конспекта",
    )?;
    tracing::info!(
        "summary model warmup finished for {model_label} in {:.2}s",
        started.elapsed().as_secs_f64()
    );
    let cache_dir = paths::qwen_cache_dir(app)?;
    write_ready_marker_for(&cache_dir, spec)?;
    Ok(())
}

pub fn generate_summary(
    app: &AppHandle,
    transcript: &str,
    cancel: Arc<CancelToken>,
) -> Result<String> {
    let system_prompt = prompts::SUMMARY_SYSTEM_PROMPT;
    let user_prompt = prompts::build_summary_user_prompt(transcript);
    let started = Instant::now();
    let spec = selected_model(app);

    let text = match generate_summary_with_server(app, system_prompt, &user_prompt, cancel.clone())
    {
        Ok(text) => {
            tracing::info!(
                "summary warm server generation finished in {:.2}s for {} chars",
                started.elapsed().as_secs_f64(),
                transcript.chars().count()
            );
            text
        }
        Err(e) => {
            if cancel.is_cancelled() {
                return Err(anyhow!("cancelled"));
            }
            tracing::warn!(
                "summary warm server failed after {:.2}s, falling back to per-job summary runtime for {}: {e:#}",
                started.elapsed().as_secs_f64(),
                spec.id
            );
            run_summary_generate(
                app,
                spec,
                MlxLmGenerateRequest {
                    system_prompt: Some(system_prompt),
                    prompt_stdin: Some(&user_prompt),
                    max_tokens: SUMMARY_MAX_TOKENS,
                    temp: Some(SUMMARY_TEMP),
                    top_p: Some(SUMMARY_TOP_P),
                    ..MlxLmGenerateRequest::default()
                },
                cancel,
                "Модель конспекта завершилась с ошибкой",
            )?
        }
    };
    if text.is_empty() {
        return Err(anyhow!("Модель конспекта вернула пустой ответ"));
    }
    tracing::info!(
        "summary generation finished in {:.2}s for {} chars",
        started.elapsed().as_secs_f64(),
        transcript.chars().count()
    );
    Ok(text)
}

fn generate_summary_with_server(
    app: &AppHandle,
    system_prompt: &str,
    user_prompt: &str,
    cancel: Arc<CancelToken>,
) -> Result<String> {
    let (url, pid, reused_server) = ensure_summary_server(app, cancel.clone())?;
    let request_started = Instant::now();
    let text = post_to_summary_server(&url, pid, system_prompt, user_prompt, cancel)?;
    tracing::info!(
        "summary warm server request finished in {:.2}s (reused_server: {reused_server})",
        request_started.elapsed().as_secs_f64()
    );
    Ok(text)
}

fn ensure_summary_server(app: &AppHandle, cancel: Arc<CancelToken>) -> Result<(String, u32, bool)> {
    let mut guard = SUMMARY_SERVER.lock();
    let spec = selected_model(app);
    if let Some(server) = guard.as_mut() {
        if server.model_id == spec.id
            && server.runtime == spec.runtime
            && server.child.try_wait()?.is_none()
            && health_ok(server.port)
        {
            return Ok((server.url(), server.pid(), true));
        }
    }

    if let Some(mut old) = guard.take() {
        old.stop();
    }

    if cancel.is_cancelled() {
        anyhow::bail!("cancelled");
    }

    let python = resolve_python(app)?;
    let cache_dir = paths::qwen_cache_dir(app)?;
    let port = find_free_port()?;

    let server_start = Instant::now();
    let mut child = build_summary_server_command(&python, &cache_dir, spec, port)
        .spawn()
        .context("Не удалось запустить сервер модели конспекта")?;

    let pid = child.id();
    cancel.register_pid(pid);
    let wait_result = wait_for_server(&mut child, port, &cancel);
    cancel.unregister_pid(pid);
    if let Err(e) = wait_result {
        let _ = child.kill();
        let _ = child.wait();
        return Err(e);
    }

    tracing::info!(
        "summary warm server started for {} on port {port} in {:.2}s",
        spec.id,
        server_start.elapsed().as_secs_f64()
    );

    *guard = Some(SummaryServer {
        model_id: spec.id,
        runtime: spec.runtime,
        port,
        child,
    });
    let server = guard.as_ref().expect("summary server just inserted");
    Ok((server.url(), server.pid(), false))
}

pub fn stop_server() {
    if let Some(mut server) = SUMMARY_SERVER.lock().take() {
        server.stop();
    }
}

pub fn preload_server(app: AppHandle) {
    tauri::async_runtime::spawn_blocking(move || {
        if !is_ready(&app) {
            return;
        }
        let cancel = CancelToken::new();
        match ensure_summary_server(&app, cancel) {
            Ok((url, _, reused)) => {
                if let Err(e) = warm_summary_server(&url) {
                    tracing::warn!("summary warm server preload request failed: {e:#}");
                    return;
                }
                tracing::info!("summary warm server preload complete (reused_server: {reused})");
            }
            Err(e) => {
                tracing::warn!("summary warm server preload failed: {e:#}");
            }
        }
    });
}

fn post_to_summary_server(
    url: &str,
    server_pid: u32,
    system_prompt: &str,
    user_prompt: &str,
    cancel: Arc<CancelToken>,
) -> Result<String> {
    if cancel.is_cancelled() {
        anyhow::bail!("cancelled");
    }

    cancel.register_pid(server_pid);
    let result = post_to_summary_server_inner(url, system_prompt, user_prompt, cancel.clone());
    cancel.unregister_pid(server_pid);
    result
}

fn warm_summary_server(url: &str) -> Result<()> {
    let client = server_client()?;
    let response = client
        .post(format!("{url}/v1/chat/completions"))
        .json(&SummaryServerRequest {
            messages: vec![SummaryChatMessage {
                role: "user",
                content: "Привет",
            }],
            max_tokens: 4,
            temperature: SUMMARY_TEMP,
            top_p: SUMMARY_TOP_P,
            stream: false,
        })
        .send()
        .context("Не удалось прогреть сервер модели конспекта")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(anyhow!(
            "Сервер модели конспекта на warmup вернул {status}: {body}"
        ));
    }
    let _ = response.bytes()?;
    Ok(())
}

fn post_to_summary_server_inner(
    url: &str,
    system_prompt: &str,
    user_prompt: &str,
    cancel: Arc<CancelToken>,
) -> Result<String> {
    let client = server_client()?;
    let response = client
        .post(format!("{url}/v1/chat/completions"))
        .json(&SummaryServerRequest {
            messages: vec![
                SummaryChatMessage {
                    role: "system",
                    content: system_prompt,
                },
                SummaryChatMessage {
                    role: "user",
                    content: user_prompt,
                },
            ],
            max_tokens: SUMMARY_MAX_TOKENS,
            temperature: SUMMARY_TEMP,
            top_p: SUMMARY_TOP_P,
            stream: true,
        })
        .send()
        .context("Не удалось отправить запрос в mlx-lm server")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(anyhow!("Сервер модели конспекта вернул {status}: {body}"));
    }

    read_summary_stream(response, cancel)
}

#[derive(Serialize)]
struct SummaryServerRequest<'a> {
    messages: Vec<SummaryChatMessage<'a>>,
    max_tokens: u32,
    temperature: f32,
    top_p: f32,
    stream: bool,
}

#[derive(Serialize)]
struct SummaryChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct SummaryStreamChunk {
    choices: Vec<SummaryStreamChoice>,
}

#[derive(Deserialize)]
struct SummaryStreamChoice {
    delta: Option<SummaryStreamDelta>,
}

#[derive(Deserialize)]
struct SummaryStreamDelta {
    content: Option<String>,
}

fn read_summary_stream(
    response: reqwest::blocking::Response,
    cancel: Arc<CancelToken>,
) -> Result<String> {
    let mut reader = BufReader::new(response);
    let mut line = String::new();
    let mut output = String::new();

    loop {
        if cancel.is_cancelled() {
            anyhow::bail!("cancelled");
        }

        line.clear();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(':') {
            continue;
        }
        let Some(data) = trimmed.strip_prefix("data:") else {
            continue;
        };
        if append_summary_stream_event(data.trim(), &mut output)? {
            break;
        }
    }

    Ok(output.trim().to_string())
}

fn append_summary_stream_event(data: &str, output: &mut String) -> Result<bool> {
    if data == "[DONE]" {
        return Ok(true);
    }
    let chunk: SummaryStreamChunk =
        serde_json::from_str(data).context("mlx-lm server вернул невалидный streaming JSON")?;
    for choice in chunk.choices {
        if let Some(delta) = choice.delta {
            if let Some(content) = delta.content {
                output.push_str(&content);
            }
        }
    }
    Ok(false)
}

struct MlxLmGenerateRequest<'a> {
    system_prompt: Option<&'a str>,
    prompt_arg: Option<&'a str>,
    prompt_stdin: Option<&'a str>,
    max_tokens: u32,
    temp: Option<f32>,
    top_p: Option<f32>,
}

impl Default for MlxLmGenerateRequest<'_> {
    fn default() -> Self {
        Self {
            system_prompt: None,
            prompt_arg: None,
            prompt_stdin: None,
            max_tokens: SUMMARY_MAX_TOKENS,
            temp: None,
            top_p: None,
        }
    }
}

fn run_summary_generate(
    app: &AppHandle,
    spec: &SummaryModelSpec,
    request: MlxLmGenerateRequest<'_>,
    cancel: Arc<CancelToken>,
    error_prefix: &str,
) -> Result<String> {
    let python = resolve_python(app)?;
    let cache_dir = paths::qwen_cache_dir(app)?;

    let mut child = build_summary_generate_command(&python, &cache_dir, spec, &request)
        .spawn()
        .context("Не удалось запустить модель конспекта")?;

    let pid = child.id();
    cancel.register_pid(pid);

    if let (SummaryRuntime::MlxLm, Some(prompt)) = (spec.runtime, request.prompt_stdin) {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Модель конспекта не открыла stdin"))?;
        stdin
            .write_all(prompt.as_bytes())
            .context("Не удалось передать промпт в stdin модели конспекта")?;
        drop(stdin);
    }

    let output = child.wait_with_output()?;
    cancel.unregister_pid(pid);

    if !output.status.success() {
        if cancel.is_cancelled() {
            return Err(anyhow!("cancelled"));
        }
        return Err(command_error(
            error_prefix,
            &output.stderr,
            output.status.code(),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn build_summary_generate_command(
    python: &Path,
    cache_dir: &Path,
    spec: &SummaryModelSpec,
    request: &MlxLmGenerateRequest<'_>,
) -> Command {
    match spec.runtime {
        SummaryRuntime::MlxLm => build_mlx_lm_generate_command(python, cache_dir, spec, request),
        SummaryRuntime::MlxVlm => build_mlx_vlm_generate_command(python, cache_dir, spec, request),
    }
}

fn build_mlx_lm_generate_command(
    python: &Path,
    cache_dir: &Path,
    spec: &SummaryModelSpec,
    request: &MlxLmGenerateRequest<'_>,
) -> Command {
    let mut command = mlx_lm_command(python, cache_dir);
    command.arg("generate").arg("--model").arg(spec.repo);
    if let Some(system_prompt) = request.system_prompt {
        command.arg("--system-prompt").arg(system_prompt);
    }
    if let Some(prompt_arg) = request.prompt_arg {
        command.arg("--prompt").arg(prompt_arg);
    } else if request.prompt_stdin.is_some() {
        command.arg("--prompt").arg("-");
        command.stdin(Stdio::piped());
    }
    command
        .arg("--max-tokens")
        .arg(request.max_tokens.to_string())
        .arg("--verbose")
        .arg("False");
    if let Some(temp) = request.temp {
        command.arg("--temp").arg(temp.to_string());
    }
    if let Some(top_p) = request.top_p {
        command.arg("--top-p").arg(top_p.to_string());
    }
    command
}

fn build_mlx_vlm_generate_command(
    python: &Path,
    cache_dir: &Path,
    spec: &SummaryModelSpec,
    request: &MlxLmGenerateRequest<'_>,
) -> Command {
    let mut command = mlx_vlm_generate_command(python, cache_dir);
    command.arg("--model").arg(spec.repo);
    if let Some(system_prompt) = request.system_prompt {
        command.arg("--system").arg(system_prompt);
    }
    if let Some(prompt_arg) = request.prompt_arg {
        command.arg("--prompt").arg(prompt_arg);
    } else if let Some(prompt_stdin) = request.prompt_stdin {
        command.arg("--prompt").arg(prompt_stdin);
    }
    command
        .arg("--max-tokens")
        .arg(request.max_tokens.to_string())
        .arg("--temperature")
        .arg(request.temp.unwrap_or(SUMMARY_TEMP).to_string())
        .arg("--verbose");
    if let Some(top_p) = request.top_p {
        command.arg("--top-p").arg(top_p.to_string());
    }
    command
}

fn build_summary_server_command(
    python: &Path,
    cache_dir: &Path,
    spec: &SummaryModelSpec,
    port: u16,
) -> Command {
    match spec.runtime {
        SummaryRuntime::MlxLm => build_mlx_lm_server_command(python, cache_dir, spec, port),
        SummaryRuntime::MlxVlm => build_mlx_vlm_server_command(python, cache_dir, spec, port),
    }
}

fn build_mlx_lm_server_command(
    python: &Path,
    cache_dir: &Path,
    spec: &SummaryModelSpec,
    port: u16,
) -> Command {
    let mut command = mlx_lm_command(python, cache_dir);
    command
        .arg("server")
        .arg("--model")
        .arg(spec.repo)
        .arg("--host")
        .arg(SERVER_HOST)
        .arg("--port")
        .arg(port.to_string())
        .arg("--allowed-origins")
        .arg("tauri://localhost,http://tauri.localhost")
        .arg("--log-level")
        .arg("ERROR")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

fn build_mlx_vlm_server_command(
    python: &Path,
    cache_dir: &Path,
    spec: &SummaryModelSpec,
    port: u16,
) -> Command {
    let mut command = mlx_vlm_server_command(python, cache_dir);
    command
        .arg("--model")
        .arg(spec.repo)
        .arg("--host")
        .arg(SERVER_HOST)
        .arg("--port")
        .arg(port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

fn mlx_lm_command(python: &Path, cache_dir: &Path) -> Command {
    let mut cmd = Command::new(python);
    cmd.arg("-m").arg("mlx_lm");
    cmd.env("HF_HOME", cache_dir)
        .env("HF_HUB_DISABLE_TELEMETRY", "1")
        .env("HF_HUB_DISABLE_XET", "1")
        .env("PYTHONUNBUFFERED", "1");
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

fn mlx_vlm_generate_command(python: &Path, cache_dir: &Path) -> Command {
    mlx_vlm_command(python, cache_dir, "generate")
}

fn mlx_vlm_server_command(python: &Path, cache_dir: &Path) -> Command {
    mlx_vlm_command(python, cache_dir, "server")
}

fn mlx_vlm_command(python: &Path, cache_dir: &Path, subcommand: &str) -> Command {
    let mut cmd = Command::new(python);
    cmd.arg("-m").arg("mlx_vlm").arg(subcommand);
    cmd.env("HF_HOME", cache_dir)
        .env("HF_HUB_DISABLE_TELEMETRY", "1")
        .env("HF_HUB_DISABLE_XET", "1")
        .env("PYTHONUNBUFFERED", "1");
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

fn wait_for_server(child: &mut Child, port: u16, cancel: &CancelToken) -> Result<()> {
    let started = Instant::now();
    while started.elapsed() < SERVER_START_TIMEOUT {
        if cancel.is_cancelled() {
            anyhow::bail!("cancelled");
        }
        if let Some(status) = child.try_wait()? {
            return Err(anyhow!("mlx-lm server завершился при запуске: {status}"));
        }
        if health_ok(port) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Err(anyhow!("mlx-lm server не успел запуститься"))
}

fn health_ok(port: u16) -> bool {
    let Ok(client) = health_client() else {
        return false;
    };
    client
        .get(format!("http://{SERVER_HOST}:{port}/health"))
        .send()
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

fn server_client() -> Result<&'static reqwest::blocking::Client> {
    SERVER_CLIENT
        .as_ref()
        .map_err(|e| anyhow!("Не удалось создать HTTP-клиент mlx-lm server: {e}"))
}

fn health_client() -> Result<&'static reqwest::blocking::Client> {
    HEALTH_CLIENT
        .as_ref()
        .map_err(|e| anyhow!("Не удалось создать health-клиент mlx-lm server: {e}"))
}

fn find_free_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind((SERVER_HOST, 0))?;
    Ok(listener.local_addr()?.port())
}

fn resolve_python(app: &AppHandle) -> Result<PathBuf> {
    if let Some(path) = env::var_os("PARROT_QWEN_PYTHON") {
        let p = PathBuf::from(path);
        if p.exists() {
            return Ok(p);
        }
    }
    if let Some(path) =
        env::var_os("PARROT_QWEN_BIN").or_else(|| env::var_os("AUDIO_TO_TEXT_QWEN_BIN"))
    {
        let p = PathBuf::from(path);
        if let Some(parent) = p.parent() {
            let py = parent.join("python");
            if py.exists() {
                return Ok(py);
            }
        }
    }

    // User-space venv created by `setup_summarizer_env` Tauri command —
    // works for users who installed Parrot from the .dmg and have no repo.
    if let Ok(env_dir) = paths::qwen_env_dir(app) {
        let py = env_dir.join("bin/python");
        if py.exists() {
            return Ok(py);
        }
    }

    // Repo-local venv created by `tools/setup_qwen_mlx.sh` — the dev path.
    for root in candidate_roots() {
        let path = root.join(".qwen-mlx/venv/bin/python");
        if path.exists() {
            return Ok(path);
        }
    }

    Err(anyhow!(
        "Python venv для конспекта не установлен. Нажмите «Установить окружение» в настройках."
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

/// Download + verify + extract astral-sh/python-build-standalone if not yet
/// installed. Returns path to the extracted `bin/python3.12`. Idempotent.
fn ensure_standalone_python<F: Fn(&str) + Send + Sync>(
    app: &AppHandle,
    on_progress: &F,
) -> Result<PathBuf> {
    let python_dir = paths::qwen_python_dir(app)?;
    let python_bin = python_dir.join("bin/python3.12");

    if python_bin.exists() {
        return Ok(python_bin);
    }

    let parent = python_dir
        .parent()
        .ok_or_else(|| anyhow!("Невалидный путь для Python"))?;
    std::fs::create_dir_all(parent).context("Не удалось создать каталог .qwen-mlx")?;

    // Download to a temp file inside the same parent (atomic-ish: same FS,
    // we move/rename only after verification + extraction succeed).
    let tarball_path = parent.join("python-download.tar.gz");
    on_progress(&format!(
        "Скачиваю Python 3.12 (~{} МБ)…",
        STANDALONE_PYTHON_BYTES / 1_048_576
    ));

    download_with_progress(STANDALONE_PYTHON_URL, &tarball_path, on_progress)
        .context("Не удалось скачать Python")?;

    on_progress("Проверяю SHA256…");
    let actual_sha = sha256_file(&tarball_path).context("Не удалось посчитать SHA256")?;
    if !actual_sha.eq_ignore_ascii_case(STANDALONE_PYTHON_SHA256) {
        let _ = std::fs::remove_file(&tarball_path);
        return Err(anyhow!(
            "SHA256 скачанного Python не совпадает (ожидалось {}, получено {}). \
             Возможно, файл повреждён — попробуйте ещё раз.",
            STANDALONE_PYTHON_SHA256,
            actual_sha
        ));
    }

    on_progress("Распаковываю Python…");
    extract_tar_gz(&tarball_path, parent).context("Не удалось распаковать Python")?;
    let _ = std::fs::remove_file(&tarball_path);

    if !python_bin.exists() {
        return Err(anyhow!(
            "Python распакован, но {} не найден. Архив повреждён или формат изменился.",
            python_bin.display()
        ));
    }

    // Strip macOS quarantine xattr so the freshly extracted binaries can run
    // without Gatekeeper prompts. Best-effort; ignore failures.
    let _ = Command::new("xattr")
        .args(["-dr", "com.apple.quarantine"])
        .arg(&python_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    Ok(python_bin)
}

fn download_with_progress<F: Fn(&str) + Send + Sync>(
    url: &str,
    dest: &Path,
    on_progress: &F,
) -> Result<()> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(DOWNLOAD_CONNECT_TIMEOUT)
        .timeout(DOWNLOAD_TOTAL_TIMEOUT)
        .build()?;
    let mut response = client
        .get(url)
        .send()
        .with_context(|| format!("HTTP GET failed: {url}"))?
        .error_for_status()?;
    let total = response.content_length().unwrap_or(STANDALONE_PYTHON_BYTES);
    let mut file = File::create(dest)
        .with_context(|| format!("Не удалось создать файл {}", dest.display()))?;
    let mut downloaded: u64 = 0;
    let mut buf = [0u8; 65_536];
    let mut last_pct: i32 = -1;
    loop {
        let n = response.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        downloaded += n as u64;
        let pct = ((downloaded as f64 / total as f64) * 100.0) as i32;
        if pct != last_pct && pct % 5 == 0 {
            on_progress(&format!(
                "Скачиваю Python: {} / {} МБ",
                downloaded / 1_048_576,
                total / 1_048_576
            ));
            last_pct = pct;
        }
    }
    file.flush()?;
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65_536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn extract_tar_gz(archive: &Path, dest_parent: &Path) -> Result<()> {
    let file = File::open(archive)?;
    let gz = GzDecoder::new(file);
    let mut tar = Archive::new(gz);
    tar.set_preserve_permissions(true);
    tar.unpack(dest_parent)?;
    Ok(())
}

/// Download standalone Python and create the user-space venv if needed.
/// Returns the venv `bin/python`. Idempotent.
pub(crate) fn ensure_user_python_venv<F: Fn(&str) + Send + Sync>(
    app: &AppHandle,
    on_progress: F,
) -> Result<PathBuf> {
    let python_bin = ensure_standalone_python(app, &on_progress)?;
    let env_dir = paths::qwen_env_dir(app)?;
    let venv_python = env_dir.join("bin/python");

    if let Some(parent) = env_dir.parent() {
        std::fs::create_dir_all(parent).context("Не удалось создать каталог окружения")?;
    }

    if !venv_python.exists() {
        on_progress("Создаю Python venv…");
        let out = Command::new(&python_bin)
            .arg("-m")
            .arg("venv")
            .arg(&env_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .context("Не удалось запустить python -m venv")?;
        if !out.status.success() {
            return Err(command_error(
                "Не удалось создать venv",
                &out.stderr,
                out.status.code(),
            ));
        }
    } else {
        on_progress("Использую существующий venv");
    }

    Ok(venv_python)
}

/// Bootstrap the user-space MLX venv: ensure Python is downloaded, create
/// the venv, upgrade pip, install MLX runtimes. Idempotent — if everything is
/// already present, returns quickly. Streams progress lines via `on_progress`.
pub fn install_env<F: Fn(&str) + Send + Sync>(app: &AppHandle, on_progress: F) -> Result<()> {
    let venv_python = ensure_user_python_venv(app, &on_progress)?;

    // Step 2 — upgrade pip (best-effort; skip failure to keep going).
    on_progress("Обновляю pip…");
    let _ = Command::new(&venv_python)
        .args(["-m", "pip", "install", "--upgrade", "pip"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    // Step 3 — install MLX runtimes for Qwen and Gemma summaries.
    on_progress("Устанавливаю MLX для Qwen и Gemma…");
    let out = Command::new(&venv_python)
        .args([
            "-m",
            "pip",
            "install",
            "--disable-pip-version-check",
            "mlx==0.31.1",
            "mlx-lm==0.31.2",
            "mlx-vlm==0.4.3",
            "parakeet-mlx==0.5.2",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("Не удалось запустить pip install mlx-lm/mlx-vlm")?;
    if !out.status.success() {
        return Err(command_error(
            "pip install mlx-lm/mlx-vlm завершился с ошибкой",
            &out.stderr,
            out.status.code(),
        ));
    }

    // Step 4 — sanity check: import both runtimes.
    on_progress("Проверяю установку…");
    let out = Command::new(&venv_python)
        .args([
            "-c",
            "import mlx_lm, mlx_vlm; print('mlx_lm ok'); print('mlx_vlm ok')",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("Не удалось проверить mlx_lm/mlx_vlm")?;
    if !out.status.success() {
        return Err(command_error(
            "mlx_lm или mlx_vlm не импортируется после установки",
            &out.stderr,
            out.status.code(),
        ));
    }

    on_progress("Готово");
    Ok(())
}

fn ready_marker_exists_for(cache_dir: &Path, spec: &SummaryModelSpec) -> bool {
    if cache_dir.join(spec.ready_marker).is_file() {
        return true;
    }
    spec.id == summarizer_models::QWEN3_4B_SUMMARY.id
        && cache_dir.join(LEGACY_QWEN_READY_MARKER).is_file()
}

fn write_ready_marker_for(cache_dir: &Path, spec: &SummaryModelSpec) -> Result<()> {
    std::fs::write(cache_dir.join(spec.ready_marker), b"ok")?;
    Ok(())
}

fn remove_ready_marker_for(cache_dir: &Path, spec: &SummaryModelSpec) {
    let _ = std::fs::remove_file(cache_dir.join(spec.ready_marker));
    if spec.id == summarizer_models::QWEN3_4B_SUMMARY.id {
        let _ = std::fs::remove_file(cache_dir.join(LEGACY_QWEN_READY_MARKER));
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "parrot-summary-ready-test-{}-{}",
            name,
            std::process::id()
        ))
    }

    #[test]
    fn ready_marker_controls_summary_readiness_per_model() {
        let dir = temp_dir("marker");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");

        assert!(!ready_marker_exists_for(
            &dir,
            &crate::summarizer_models::GEMMA4_E2B_SUMMARY
        ));
        write_ready_marker_for(&dir, &crate::summarizer_models::GEMMA4_E2B_SUMMARY)
            .expect("write marker");
        assert!(ready_marker_exists_for(
            &dir,
            &crate::summarizer_models::GEMMA4_E2B_SUMMARY
        ));
        remove_ready_marker_for(&dir, &crate::summarizer_models::GEMMA4_E2B_SUMMARY);
        assert!(!ready_marker_exists_for(
            &dir,
            &crate::summarizer_models::GEMMA4_E2B_SUMMARY
        ));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn qwen_legacy_marker_should_keep_summary_readiness() {
        let dir = temp_dir("legacy-marker");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::fs::write(dir.join(".parrot-ready-summary"), b"ok").expect("legacy marker");

        assert!(ready_marker_exists_for(
            &dir,
            &crate::summarizer_models::QWEN3_4B_SUMMARY
        ));
        assert!(!ready_marker_exists_for(
            &dir,
            &crate::summarizer_models::GEMMA4_E2B_SUMMARY
        ));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn summary_stream_events_append_content_until_done() {
        let mut output = String::new();

        let done = append_summary_stream_event(
            r###"{"choices":[{"delta":{"content":"## Резюме\n"}}]}"###,
            &mut output,
        )
        .expect("append first chunk");
        assert!(!done);

        let done = append_summary_stream_event(
            r#"{"choices":[{"delta":{"content":"Готово"}}]}"#,
            &mut output,
        )
        .expect("append second chunk");
        assert!(!done);

        let done = append_summary_stream_event("[DONE]", &mut output).expect("done chunk");
        assert!(done);
        assert_eq!(output, "## Резюме\nГотово");
    }

    #[test]
    fn qwen_summary_server_command_uses_mlx_lm() {
        let command = build_mlx_lm_server_command(
            Path::new("/tmp/python"),
            Path::new("/tmp/cache"),
            &crate::summarizer_models::QWEN3_4B_SUMMARY,
            18181,
        );
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert!(args.windows(2).any(|pair| pair == ["-m", "mlx_lm"]));
        assert!(args.iter().any(|arg| arg == "server"));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--model", crate::summarizer_models::QWEN3_4B_SUMMARY.repo]));
        assert!(args.windows(2).any(|pair| pair == ["--host", SERVER_HOST]));
        assert!(args.windows(2).any(|pair| pair == ["--port", "18181"]));
    }

    #[test]
    fn gemma_summary_server_command_uses_mlx_vlm() {
        let command = build_mlx_vlm_server_command(
            Path::new("/tmp/python"),
            Path::new("/tmp/cache"),
            &crate::summarizer_models::GEMMA4_E2B_SUMMARY,
            18182,
        );
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert!(args.windows(2).any(|pair| pair == ["-m", "mlx_vlm"]));
        assert!(args.iter().any(|arg| arg == "server"));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--model", crate::summarizer_models::GEMMA4_E2B_SUMMARY.repo]));
        assert!(args.windows(2).any(|pair| pair == ["--host", SERVER_HOST]));
        assert!(args.windows(2).any(|pair| pair == ["--port", "18182"]));
    }
}
