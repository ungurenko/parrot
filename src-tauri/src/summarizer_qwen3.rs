use anyhow::{anyhow, Context, Result};
use flate2::read::GzDecoder;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::env;
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tar::Archive;
use tauri::AppHandle;

#[cfg(unix)]
use std::os::fd::AsRawFd;

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
const SERVER_LOCK_FILE: &str = "summary-server.lock";
const SERVER_REGISTRY_FILE: &str = "summary-server.json";
const SHARED_SERVER_WAIT_TIMEOUT: Duration = Duration::from_secs(5);
const SERVER_STOP_TIMEOUT: Duration = Duration::from_secs(5);
const SERVER_REGISTRY_VERSION: u8 = 1;

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SummaryServerRecord {
    registry_version: u8,
    pid: u32,
    port: u16,
    model_id: String,
    model: String,
    runtime: String,
    python: String,
    process_start_time: u64,
    owner_pid: u32,
    owner_start_time: Option<u64>,
}

struct SummaryServerLock {
    _file: File,
}

impl SummaryServerLock {
    fn try_acquire(path: &Path) -> Result<Option<Self>> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .with_context(|| format!("Не удалось открыть lock-файл {}", path.display()))?;

        #[cfg(unix)]
        {
            // The descriptor stays open for the whole lifetime of the owned
            // server. The kernel releases the lock if Parrot crashes; Rust's
            // standard file descriptors are close-on-exec, so Python does not
            // accidentally keep the lock after the parent exits.
            let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
            if result != 0 {
                let error = io::Error::last_os_error();
                if matches!(
                    error.kind(),
                    ErrorKind::WouldBlock | ErrorKind::PermissionDenied
                ) {
                    return Ok(None);
                }
                return Err(error).context("Не удалось захватить lock-файл summary-сервера");
            }
        }

        Ok(Some(Self { _file: file }))
    }
}

struct SummaryServer {
    record: SummaryServerRecord,
    child: Option<Child>,
    owner_lock: Option<SummaryServerLock>,
    registry_path: PathBuf,
}

impl SummaryServer {
    fn url(&self) -> String {
        format!("http://{SERVER_HOST}:{}", self.record.port)
    }

    fn pid(&self) -> u32 {
        self.record.pid
    }

    fn matches_spec(&self, spec: &SummaryModelSpec) -> bool {
        self.record.model_id == spec.id
            && self.record.model == spec.repo
            && self.record.runtime == runtime_name(spec.runtime)
    }

    fn is_owned(&self) -> bool {
        self.child.is_some() && self.owner_lock.is_some()
    }

    fn is_process_alive(&mut self) -> Result<bool> {
        if let Some(child) = self.child.as_mut() {
            return Ok(child.try_wait()?.is_none());
        }
        Ok(process_start_time(self.record.pid) == Some(self.record.process_start_time))
    }

    fn stop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let stop_result = stop_child(child);
            tracing::info!(
                "summary server stopped (pid={}, port={}, model={}, owned={}, result={})",
                self.record.pid,
                self.record.port,
                self.record.model,
                self.is_owned(),
                stop_result
            );
            remove_registry_if_matches(&self.registry_path, &self.record);
        } else {
            tracing::debug!(
                "summary server handle released without stopping shared server (pid={}, port={}, model={})",
                self.record.pid,
                self.record.port,
                self.record.model
            );
        }
        self.owner_lock.take();
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
    let text = match post_to_summary_server(&url, pid, system_prompt, user_prompt, cancel) {
        Ok(text) => text,
        Err(error) => {
            // A failed request can mean that the Python server is wedged. Do
            // not leave it resident while the per-job fallback loads another
            // copy of the model.
            retire_summary_server(pid);
            return Err(error);
        }
    };
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
        if server.matches_spec(spec) && server.is_process_alive()? && health_ok(server.record.port)
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

    let (registry_path, lock_path) = server_registry_paths(app)?;
    let Some(owner_lock) = SummaryServerLock::try_acquire(&lock_path)? else {
        if let Some(record) = wait_for_shared_server(&registry_path, spec) {
            tracing::info!(
                "reusing summary server owned by another Parrot instance (pid={}, port={}, model={})",
                record.pid,
                record.port,
                record.model
            );
            let server = SummaryServer {
                record,
                child: None,
                owner_lock: None,
                registry_path,
            };
            let url = server.url();
            let pid = server.pid();
            *guard = Some(server);
            return Ok((url, pid, true));
        }
        anyhow::bail!(
            "Другой экземпляр Parrot уже владеет сервером конспекта; второй сервер не запускается"
        );
    };

    // A previous Parrot may have crashed after the child was spawned. The
    // released lock proves that no live owner remains, so remove only the
    // exact process described by this app's registry before starting anew.
    cleanup_registry_record(&registry_path);

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
        let stop_result = stop_child(&mut child);
        tracing::warn!(
            "summary server failed during startup (pid={pid}, port={port}, stop_result={stop_result}): {e:#}"
        );
        return Err(e);
    }

    let server_start_time = process_start_time(pid).ok_or_else(|| {
        let stop_result = stop_child(&mut child);
        tracing::warn!(
            "summary server PID {pid} has no readable start time; stopped untracked server (result={stop_result})"
        );
        anyhow!("Не удалось определить время запуска summary-сервера")
    })?;

    let record = SummaryServerRecord {
        registry_version: SERVER_REGISTRY_VERSION,
        pid,
        port,
        model_id: spec.id.to_string(),
        model: spec.repo.to_string(),
        runtime: runtime_name(spec.runtime).to_string(),
        python: python.to_string_lossy().to_string(),
        process_start_time: server_start_time,
        owner_pid: std::process::id(),
        owner_start_time: process_start_time(std::process::id()),
    };
    if let Err(error) = write_registry(&registry_path, &record) {
        let stop_result = stop_child(&mut child);
        tracing::warn!(
            "summary server registry write failed (pid={pid}, port={port}, stop_result={stop_result}): {error:#}"
        );
        return Err(error);
    }

    tracing::info!(
        "summary warm server started (pid={pid}, port={port}, model={}, runtime={}, reused=false) in {:.2}s",
        spec.id,
        runtime_name(spec.runtime),
        server_start.elapsed().as_secs_f64()
    );

    *guard = Some(SummaryServer {
        record,
        child: Some(child),
        owner_lock: Some(owner_lock),
        registry_path,
    });
    let server = guard.as_ref().expect("summary server just inserted");
    Ok((server.url(), server.pid(), false))
}

pub fn stop_server() {
    if let Some(mut server) = SUMMARY_SERVER.lock().take() {
        server.stop();
    }
}

fn retire_summary_server(pid: u32) {
    let mut guard = SUMMARY_SERVER.lock();
    if guard.as_ref().map(SummaryServer::pid) != Some(pid) {
        return;
    }
    if let Some(mut server) = guard.take() {
        tracing::warn!(
            "retiring unhealthy summary server after request failure (pid={}, port={}, model={})",
            server.record.pid,
            server.record.port,
            server.record.model
        );
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
                    stop_server();
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

fn runtime_name(runtime: SummaryRuntime) -> &'static str {
    match runtime {
        SummaryRuntime::MlxLm => "mlx_lm",
        SummaryRuntime::MlxVlm => "mlx_vlm",
    }
}

fn server_registry_paths(app: &AppHandle) -> Result<(PathBuf, PathBuf)> {
    let dir = paths::app_data_dir(app)?;
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Не удалось создать каталог {}", dir.display()))?;
    Ok((dir.join(SERVER_REGISTRY_FILE), dir.join(SERVER_LOCK_FILE)))
}

fn read_registry(path: &Path) -> Result<Option<SummaryServerRecord>> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .with_context(|| format!("Повреждён registry summary-сервера {}", path.display()))
            .map(Some),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| {
            format!(
                "Не удалось прочитать registry summary-сервера {}",
                path.display()
            )
        }),
    }
}

fn write_registry(path: &Path, record: &SummaryServerRecord) -> Result<()> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let temp_path = path.with_file_name(format!(
        "{}.tmp-{}-{nonce}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(SERVER_REGISTRY_FILE),
        std::process::id()
    ));

    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
            .with_context(|| {
                format!(
                    "Не удалось создать временный registry {}",
                    temp_path.display()
                )
            })?;
        let bytes = serde_json::to_vec_pretty(record)?;
        file.write_all(&bytes)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        std::fs::rename(&temp_path, path).with_context(|| {
            format!(
                "Не удалось атомарно заменить registry summary-сервера {}",
                path.display()
            )
        })?;
        Ok(())
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    result
}

fn remove_registry_if_matches(path: &Path, expected: &SummaryServerRecord) {
    match read_registry(path) {
        Ok(Some(current)) if current == *expected => match std::fs::remove_file(path) {
            Ok(()) => tracing::info!(
                "summary server registry removed (pid={}, port={}, model={})",
                expected.pid,
                expected.port,
                expected.model
            ),
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => tracing::warn!(
                "could not remove summary server registry {}: {error}",
                path.display()
            ),
        },
        Ok(Some(_)) => {
            tracing::debug!("summary server registry changed before cleanup; leaving it untouched")
        }
        Ok(None) => {}
        Err(error) => tracing::warn!("could not inspect summary server registry: {error:#}"),
    }
}

#[derive(Debug, Clone)]
struct ProcessSnapshot {
    pid: u32,
    ppid: u32,
    command: String,
    start_time: u64,
}

fn validate_registry_record(record: &SummaryServerRecord) -> Option<ProcessSnapshot> {
    if record.registry_version != SERVER_REGISTRY_VERSION {
        tracing::info!(
            "ignoring summary registry with unsupported version {}",
            record.registry_version
        );
        return None;
    }
    let start_time = process_start_time(record.pid)?;
    if !registry_identity_matches(record, start_time) {
        tracing::info!(
            "summary registry PID {} was reused (expected start {}, got {})",
            record.pid,
            record.process_start_time,
            start_time
        );
        return None;
    }
    let command = process_command_line(record.pid)?;
    if !summary_command_matches(
        &command,
        &record.model,
        &record.runtime,
        &record.python,
        record.port,
    ) {
        tracing::info!(
            "summary registry PID {} command no longer matches Parrot ownership",
            record.pid
        );
        return None;
    }
    Some(ProcessSnapshot {
        pid: record.pid,
        ppid: 0,
        command,
        start_time,
    })
}

fn registry_identity_matches(record: &SummaryServerRecord, actual_start_time: u64) -> bool {
    record.process_start_time == actual_start_time
}

fn wait_for_shared_server(
    registry_path: &Path,
    spec: &SummaryModelSpec,
) -> Option<SummaryServerRecord> {
    let started = Instant::now();
    while started.elapsed() < SHARED_SERVER_WAIT_TIMEOUT {
        if let Ok(Some(record)) = read_registry(registry_path) {
            if record.model_id == spec.id
                && record.model == spec.repo
                && record.runtime == runtime_name(spec.runtime)
                && validate_registry_record(&record).is_some()
                && health_ok(record.port)
            {
                return Some(record);
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    None
}

fn cleanup_registry_record(path: &Path) {
    let record = match read_registry(path) {
        Ok(Some(record)) => record,
        Ok(None) => return,
        Err(error) => {
            tracing::warn!(
                "summary server registry is unreadable; removing metadata without killing a process: {error:#}"
            );
            let _ = std::fs::remove_file(path);
            return;
        }
    };

    if let Some(snapshot) = validate_registry_record(&record) {
        let healthy = health_ok(record.port);
        tracing::warn!(
            "cleaning stale registered summary server (pid={}, port={}, model={}, runtime={}, health={})",
            record.pid,
            record.port,
            record.model,
            record.runtime,
            healthy
        );
        let _ = terminate_process(&snapshot, |command| {
            summary_command_matches(
                command,
                &record.model,
                &record.runtime,
                &record.python,
                record.port,
            )
        });
    } else {
        tracing::info!(
            "stale summary registry does not identify a matching live process; no PID was killed (pid={}, port={}, model={})",
            record.pid,
            record.port,
            record.model
        );
    }
    let _ = std::fs::remove_file(path);
}

/// Reconcile summary servers left by an older Parrot process before any new
/// warm server is preloaded. Only an exact app-owned command with PPID 1 is
/// eligible for legacy cleanup; arbitrary Python/MLX processes are ignored.
pub fn cleanup_stale_servers(app: &AppHandle) {
    if let Err(error) = cleanup_stale_servers_inner(app) {
        tracing::warn!("summary server startup cleanup failed: {error:#}");
    }
}

fn cleanup_stale_servers_inner(app: &AppHandle) -> Result<()> {
    let (registry_path, lock_path) = server_registry_paths(app)?;
    let Some(_lock) = SummaryServerLock::try_acquire(&lock_path)? else {
        tracing::info!(
            "summary server startup cleanup skipped: another Parrot instance owns the registry lock"
        );
        return Ok(());
    };

    let mut protected_pids = HashSet::new();
    let mut found = 0u32;
    let mut stopped = 0u32;

    match read_registry(&registry_path) {
        Ok(Some(record)) => {
            protected_pids.insert(record.pid);
            if let Some(snapshot) = validate_registry_record(&record) {
                let healthy = health_ok(record.port);
                tracing::warn!(
                    "cleaning registered orphan summary server (pid={}, port={}, model={}, runtime={}, health={})",
                    record.pid,
                    record.port,
                    record.model,
                    record.runtime,
                    healthy
                );
                found += 1;
                if terminate_process(&snapshot, |command| {
                    summary_command_matches(
                        command,
                        &record.model,
                        &record.runtime,
                        &record.python,
                        record.port,
                    )
                }) {
                    stopped += 1;
                }
            } else {
                tracing::info!(
                    "registered summary PID {} is not a confirmed Parrot process; it was not killed",
                    record.pid
                );
            }
            let _ = std::fs::remove_file(&registry_path);
        }
        Ok(None) => {}
        Err(error) => {
            tracing::warn!(
                "summary registry could not be read; no registry PID was killed: {error:#}"
            );
            let _ = std::fs::remove_file(&registry_path);
        }
    }

    let python_paths = summary_python_candidates(app);
    for snapshot in list_process_snapshots()? {
        if protected_pids.contains(&snapshot.pid) || snapshot.ppid != 1 {
            continue;
        }
        let Some((model, runtime, python, port)) =
            legacy_summary_metadata(&snapshot.command, &python_paths)
        else {
            continue;
        };
        found += 1;
        let healthy = health_ok(port);
        tracing::warn!(
            "cleaning legacy orphan summary server (pid={}, port={}, model={}, runtime={}, health={})",
            snapshot.pid,
            port,
            model,
            runtime,
            healthy
        );
        if terminate_process(&snapshot, |command| {
            summary_command_matches(command, &model, &runtime, &python, port)
        }) {
            stopped += 1;
        }
    }

    tracing::info!(
        "summary server startup cleanup complete (found={found}, stopped={stopped}, protected_pids={})",
        protected_pids.len()
    );
    Ok(())
}

fn summary_python_candidates(app: &AppHandle) -> Vec<String> {
    let mut paths = Vec::new();
    let mut add = |path: PathBuf| {
        let candidates = [path.clone(), path.canonicalize().unwrap_or(path)];
        for candidate in candidates {
            let value = candidate.to_string_lossy().to_string();
            if !paths.contains(&value) {
                paths.push(value);
            }
        }
    };

    if let Some(path) = env::var_os("PARROT_QWEN_PYTHON") {
        add(PathBuf::from(path));
    }
    for variable in ["PARROT_QWEN_BIN", "AUDIO_TO_TEXT_QWEN_BIN"] {
        if let Some(path) = env::var_os(variable) {
            if let Some(parent) = PathBuf::from(path).parent() {
                add(parent.join("python"));
            }
        }
    }
    if let Ok(env_dir) = paths::qwen_env_dir(app) {
        add(env_dir.join("bin/python"));
    }
    for root in candidate_roots() {
        add(root.join(".qwen-mlx/venv/bin/python"));
    }
    paths
}

fn legacy_summary_metadata(
    command: &str,
    python_paths: &[String],
) -> Option<(String, String, String, u16)> {
    for python in python_paths {
        for spec in summarizer_models::SUPPORTED_SUMMARY_MODELS {
            let runtime = runtime_name(spec.runtime).to_string();
            let Some(port) = command_arg(command, "--port").and_then(|value| value.parse().ok())
            else {
                continue;
            };
            if summary_command_matches(command, spec.repo, &runtime, python, port) {
                return Some((spec.repo.to_string(), runtime, python.clone(), port));
            }
        }
    }
    None
}

fn summary_command_matches(
    command: &str,
    model: &str,
    runtime: &str,
    python: &str,
    port: u16,
) -> bool {
    if !command.contains(python) {
        return false;
    }
    let module = match runtime {
        "mlx_lm" => "mlx_lm",
        "mlx_vlm" => "mlx_vlm",
        _ => return false,
    };
    command.contains(&format!("-m {module} server"))
        && command_arg(command, "--model") == Some(model)
        && command_arg(command, "--host") == Some(SERVER_HOST)
        && command_arg(command, "--port").and_then(|value| value.parse().ok()) == Some(port)
}

fn command_arg<'a>(command: &'a str, name: &str) -> Option<&'a str> {
    let equals_prefix = format!("{name}=");
    if let Some(value) = command
        .split_whitespace()
        .find_map(|part| part.strip_prefix(&equals_prefix))
    {
        return Some(value);
    }
    let space_prefix = format!("{name} ");
    command
        .split_once(&space_prefix)
        .and_then(|(_, rest)| rest.split_whitespace().next())
}

fn process_command_line(pid: u32) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let bytes = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
        let command = bytes
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty())
            .map(|part| String::from_utf8_lossy(part).into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        return (!command.is_empty()).then_some(command);
    }

    #[cfg(not(target_os = "linux"))]
    {
        let output = Command::new("/bin/ps")
            .args(["-p", &pid.to_string(), "-o", "command="])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let command = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (!command.is_empty()).then_some(command)
    }
}

fn list_process_snapshots() -> Result<Vec<ProcessSnapshot>> {
    let output = Command::new("/bin/ps")
        .args(["-Ao", "pid=,ppid=,command="])
        .output()
        .context("Не удалось перечислить процессы для summary cleanup")?;
    if !output.status.success() {
        return Err(anyhow!(
            "ps завершился с кодом {}",
            output.status.code().unwrap_or(-1)
        ));
    }

    let mut snapshots = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let mut fields = line.split_whitespace();
        let Some(pid) = fields.next().and_then(|value| value.parse().ok()) else {
            continue;
        };
        let Some(ppid) = fields.next().and_then(|value| value.parse().ok()) else {
            continue;
        };
        let command = fields.collect::<Vec<_>>().join(" ");
        let Some(start_time) = process_start_time(pid) else {
            continue;
        };
        snapshots.push(ProcessSnapshot {
            pid,
            ppid,
            command,
            start_time,
        });
    }
    Ok(snapshots)
}

fn process_start_time(pid: u32) -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
        let result = unsafe {
            libc::proc_pidinfo(
                pid as libc::c_int,
                libc::PROC_PIDTBSDINFO,
                0,
                &mut info as *mut _ as *mut libc::c_void,
                size,
            )
        };
        if result != size {
            return None;
        }
        info.pbi_start_tvsec
            .checked_mul(1_000_000)
            .and_then(|seconds| seconds.checked_add(info.pbi_start_tvusec))
    }

    #[cfg(target_os = "linux")]
    {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let (_, fields) = stat.rsplit_once(") ")?;
        // After the command name, field 3 is at index 0 and field 22
        // (starttime) is therefore index 19.
        return fields.split_whitespace().nth(19)?.parse().ok();
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = pid;
        None
    }
}

fn terminate_process<F>(snapshot: &ProcessSnapshot, validator: F) -> bool
where
    F: Fn(&str) -> bool,
{
    let Some(current_command) = process_command_line(snapshot.pid) else {
        return true;
    };
    if process_start_time(snapshot.pid) != Some(snapshot.start_time) || !validator(&current_command)
    {
        tracing::warn!(
            "summary cleanup refused PID {} after identity/command changed",
            snapshot.pid
        );
        return false;
    }

    let term_result = signal_process(snapshot.pid, libc::SIGTERM);
    let deadline = Instant::now() + SERVER_STOP_TIMEOUT;
    while Instant::now() < deadline {
        if process_start_time(snapshot.pid) != Some(snapshot.start_time) {
            tracing::info!("summary orphan PID {} exited after SIGTERM", snapshot.pid);
            return true;
        }
        thread::sleep(Duration::from_millis(100));
    }

    if process_start_time(snapshot.pid) == Some(snapshot.start_time) {
        let kill_result = signal_process(snapshot.pid, libc::SIGKILL);
        tracing::warn!(
            "summary orphan PID {} did not stop after SIGTERM; SIGKILL result={kill_result:?}",
            snapshot.pid
        );
    }
    term_result.is_ok() || process_start_time(snapshot.pid) != Some(snapshot.start_time)
}

fn signal_process(pid: u32, signal: libc::c_int) -> io::Result<()> {
    #[cfg(unix)]
    {
        let result = unsafe { libc::kill(pid as libc::pid_t, signal) };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    #[cfg(not(unix))]
    {
        let _ = (pid, signal);
        Err(io::Error::new(
            ErrorKind::Unsupported,
            "process signals are unavailable on this platform",
        ))
    }
}

fn stop_child(child: &mut Child) -> String {
    let pid = child.id();
    match child.try_wait() {
        Ok(Some(status)) => return format!("already exited ({status})"),
        Ok(None) => {}
        Err(error) => return format!("wait before stop failed: {error}"),
    }

    let term_result = signal_process(pid, libc::SIGTERM);
    let deadline = Instant::now() + SERVER_STOP_TIMEOUT;
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(status)) => return format!("SIGTERM -> {status}"),
            Ok(None) => thread::sleep(Duration::from_millis(100)),
            Err(error) => return format!("wait after SIGTERM failed: {error}"),
        }
    }

    let kill_result = child.kill();
    let wait_result = child.wait();
    format!("SIGTERM={term_result:?}, SIGKILL={kill_result:?}, wait={wait_result:?}")
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

    #[test]
    fn summary_server_registry_roundtrips_all_ownership_fields() {
        let dir = temp_dir("registry-roundtrip");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join(SERVER_REGISTRY_FILE);
        let record = SummaryServerRecord {
            registry_version: SERVER_REGISTRY_VERSION,
            pid: 49192,
            port: 60732,
            model_id: "qwen3-4b".to_string(),
            model: crate::summarizer_models::QWEN3_4B_SUMMARY.repo.to_string(),
            runtime: "mlx_lm".to_string(),
            python:
                "/Users/alex/Library/Application Support/com.alexk.parrot/.qwen-mlx/venv/bin/python"
                    .to_string(),
            process_start_time: 123_456,
            owner_pid: 65442,
            owner_start_time: Some(123_400),
        };

        write_registry(&path, &record).expect("write registry");
        assert_eq!(read_registry(&path).expect("read registry"), Some(record));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn summary_server_lock_allows_only_one_owner_at_a_time() {
        let dir = temp_dir("lock-sequential");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join(SERVER_LOCK_FILE);

        let first = SummaryServerLock::try_acquire(&path)
            .expect("acquire first lock")
            .expect("first lock should be available");
        assert!(SummaryServerLock::try_acquire(&path)
            .expect("probe second lock")
            .is_none());
        drop(first);
        assert!(SummaryServerLock::try_acquire(&path)
            .expect("acquire after release")
            .is_some());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parallel_summary_server_claims_have_one_winner() {
        let dir = temp_dir("lock-parallel");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = Arc::new(dir.join(SERVER_LOCK_FILE));
        let start = Arc::new(std::sync::Barrier::new(2));
        let checked = Arc::new(std::sync::Barrier::new(2));
        let mut handles = Vec::new();

        for _ in 0..2 {
            let path = path.clone();
            let start = start.clone();
            let checked = checked.clone();
            handles.push(std::thread::spawn(move || {
                start.wait();
                let claim = SummaryServerLock::try_acquire(&path).expect("parallel lock probe");
                let won = claim.is_some();
                checked.wait();
                won
            }));
        }

        let winners = handles
            .into_iter()
            .map(|handle| handle.join().expect("join lock probe"))
            .filter(|won| *won)
            .count();
        assert_eq!(winners, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reused_pid_start_time_is_rejected() {
        let record = SummaryServerRecord {
            registry_version: SERVER_REGISTRY_VERSION,
            pid: 1,
            port: 18181,
            model_id: "qwen3-4b".to_string(),
            model: crate::summarizer_models::QWEN3_4B_SUMMARY.repo.to_string(),
            runtime: "mlx_lm".to_string(),
            python: "/tmp/python".to_string(),
            process_start_time: 10,
            owner_pid: 11,
            owner_start_time: Some(12),
        };

        assert!(registry_identity_matches(&record, 10));
        assert!(!registry_identity_matches(&record, 11));
    }

    #[test]
    fn cleanup_filter_ignores_qwen_asr_and_foreign_python_processes() {
        let python =
            "/Users/alex/Library/Application Support/com.alexk.parrot/.qwen-mlx/venv/bin/python";
        let summary_command = format!(
            "{python} -m mlx_lm server --model {} --host 127.0.0.1 --port 49192",
            crate::summarizer_models::QWEN3_4B_SUMMARY.repo
        );
        let asr_command = format!(
            "{python} -m mlx_qwen3_asr serve --model Qwen/Qwen3-ASR-0.6B --host 127.0.0.1 --port 49192"
        );
        let foreign_command = format!(
            "/usr/bin/python -m mlx_lm server --model {} --host 127.0.0.1 --port 49192",
            crate::summarizer_models::QWEN3_4B_SUMMARY.repo
        );

        assert!(summary_command_matches(
            &summary_command,
            crate::summarizer_models::QWEN3_4B_SUMMARY.repo,
            "mlx_lm",
            python,
            49192
        ));
        assert!(legacy_summary_metadata(&summary_command, &[python.to_string()]).is_some());
        assert!(legacy_summary_metadata(&asr_command, &[python.to_string()]).is_none());
        assert!(legacy_summary_metadata(&foreign_command, &[python.to_string()]).is_none());
    }
}
