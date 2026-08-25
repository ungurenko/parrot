use anyhow::{anyhow, Context, Result};
use parakeet_rs::{
    ExecutionConfig as ParakeetExecConfig, ExecutionProvider, ParakeetTDT, Transcriber,
};
use parking_lot::Mutex;
use serde::Deserialize;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

// Parakeet TDT has a ~8-10 min sequence-length limit. Five-minute chunks are
// the stable path for long lectures and interviews.
const CHUNK_SECONDS: usize = 5 * 60;
const OVERLAP_SECONDS: usize = 5;
const MIN_FINAL_CHUNK_SECONDS: usize = 30;
const SAMPLE_RATE: u32 = 16_000;

static MODEL: LazyLock<Mutex<Option<Arc<Mutex<ParakeetTDT>>>>> = LazyLock::new(|| Mutex::new(None));
static MLX_PYTHON: Mutex<Option<Option<PathBuf>>> = Mutex::new(None);
static MLX_INSTALL_STARTED: AtomicBool = AtomicBool::new(false);

const MLX_MODEL: &str = "mlx-community/parakeet-tdt-0.6b-v3";
const MLX_READY_MARKER: &str = ".parrot-ready-parakeet-mlx";
const MLX_PACKAGE: &str = "parakeet-mlx==0.5.2";

/// Persistent worker: loads the fp16 model once, then serves WAV paths on
/// stdin and answers with line-delimited JSON events. Spawning Python per
/// job used to pay interpreter startup + model load (~3-8 s) every time,
/// which dominated short files and dictation.
const MLX_WORKER_SCRIPT: &str = r#"
import json
import sys


def emit(payload):
    sys.stdout.write(json.dumps(payload, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def main():
    from parakeet_mlx import from_pretrained

    model = from_pretrained(sys.argv[1])
    emit({"event": "ready"})
    for line in sys.stdin:
        path = line.strip()
        if not path:
            continue
        if path == "SHUTDOWN":
            break
        try:
            def on_chunk(done, total):
                emit({"event": "progress", "done": done, "total": total})

            result = model.transcribe(
                path,
                chunk_duration=120.0,
                overlap_duration=15.0,
                chunk_callback=on_chunk,
            )
            emit({"event": "result", "ok": True, "text": result.text})
        except Exception as exc:
            emit({"event": "result", "ok": False, "error": str(exc)})
    emit({"event": "bye"})


main()
"#;

pub fn preload(model_dir: &Path) -> Result<()> {
    get_or_load_model(model_dir)?;
    Ok(())
}

pub fn clear_cache() {
    *MODEL.lock() = None;
}

pub fn refresh_mlx_python() {
    *MLX_PYTHON.lock() = Some(detect_mlx_python());
}

fn current_mlx_python() -> Option<PathBuf> {
    let mut guard = MLX_PYTHON.lock();
    if guard.is_none() {
        *guard = Some(detect_mlx_python());
    }
    guard.clone().flatten()
}

fn is_mlx_ready() -> bool {
    current_mlx_python().is_some()
}

/// Install Python + parakeet-mlx + download weights in the background.
/// Transcription keeps using ONNX until this finishes.
pub fn spawn_mlx_install(app: AppHandle) {
    if !crate::hardware::mlx_acceleration_allowed() || is_mlx_ready() {
        return;
    }
    if MLX_INSTALL_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn_blocking(move || {
        let emit_app = app.clone();
        let result = install_mlx_runtime(&app, |line| {
            let _ = emit_app.emit("parakeet_mlx:progress", line.to_string());
        });
        match result {
            Ok(()) => {
                refresh_mlx_python();
                tracing::info!("Parakeet MLX runtime is ready");
                if crate::hardware::mlx_acceleration_allowed() {
                    // The fp16 worker now serves transcription; drop the
                    // int8 ONNX weights instead of keeping both resident.
                    clear_cache();
                    tauri::async_runtime::spawn_blocking(|| {
                        if preload_worker() {
                            tracing::info!("Parakeet MLX worker warmed after install");
                        }
                    });
                }
                let _ = app.emit("parakeet_mlx:ready", ());
            }
            Err(e) => {
                MLX_INSTALL_STARTED.store(false, Ordering::SeqCst);
                tracing::error!("Parakeet MLX install failed: {e:#}");
                let _ = app.emit("parakeet_mlx:error", e.to_string());
            }
        }
    });
}

fn get_or_load_model(model_dir: &Path) -> Result<Arc<Mutex<ParakeetTDT>>> {
    let mut guard = MODEL.lock();
    if guard.is_none() {
        // Use all performance cores for faster inference (CPU EP, which is the only
        // reliable provider for Parakeet's dynamic-shape ONNX graph).
        let intra = std::thread::available_parallelism()
            .map(|n| n.get().min(8))
            .unwrap_or(4);
        let exec = ParakeetExecConfig {
            execution_provider: ExecutionProvider::Cpu,
            intra_threads: intra,
            inter_threads: 1,
            configure: None,
            coreml_cache_dir: None,
        };
        let model = ParakeetTDT::from_pretrained(model_dir, Some(exec))
            .map_err(|e| anyhow!("failed to load Parakeet model: {e:?}"))?;
        *guard = Some(Arc::new(Mutex::new(model)));
    }
    Ok(guard.as_ref().unwrap().clone())
}

fn detect_mlx_python() -> Option<PathBuf> {
    resolve_mlx_python(
        std::env::var_os("PARROT_PARAKEET_MLX_PYTHON").map(PathBuf::from),
        &mlx_candidate_roots(),
    )
    .filter(|python| crate::mlx_env::python_import_ok(python, "import parakeet_mlx"))
}

fn resolve_mlx_python(explicit: Option<PathBuf>, roots: &[PathBuf]) -> Option<PathBuf> {
    if let Some(path) = explicit {
        if path.is_file() {
            return Some(path);
        }
    }
    roots.iter().find_map(|root| {
        let path = root.join(".qwen-mlx/venv/bin/python");
        path.is_file().then_some(path)
    })
}

fn mlx_candidate_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join("Library/Application Support/com.alexk.parrot"));
    }
    if let Ok(current) = std::env::current_dir() {
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

fn mlx_hf_home() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("HF_HOME") {
        return Some(PathBuf::from(path));
    }
    dirs::home_dir()
        .map(|home| home.join("Library/Application Support/com.alexk.parrot/models/qwen-mlx"))
}

fn mlx_ready_marker_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join(MLX_READY_MARKER)
}

fn mlx_ready_marker_exists(cache_dir: &Path) -> bool {
    mlx_ready_marker_path(cache_dir).is_file()
}

fn write_mlx_ready_marker(cache_dir: &Path) -> Result<()> {
    std::fs::write(mlx_ready_marker_path(cache_dir), b"ok")?;
    Ok(())
}

fn install_mlx_runtime(app: &AppHandle, on_progress: impl Fn(&str) + Send + Sync) -> Result<()> {
    crate::mlx_env::with_install_lock(|| install_mlx_runtime_locked(app, &on_progress))
}

fn install_mlx_runtime_locked(
    app: &AppHandle,
    on_progress: &(impl Fn(&str) + Send + Sync),
) -> Result<()> {
    let venv_python = crate::mlx_env::ensure_user_python_venv(app, on_progress)?;
    if crate::mlx_env::python_import_ok(&venv_python, "import parakeet_mlx") {
        if let Ok(cache) = crate::paths::qwen_cache_dir(app) {
            if mlx_ready_marker_exists(&cache) {
                on_progress("Ускорение уже готово");
                return Ok(());
            }
        }
    } else {
        on_progress("Устанавливаю ускорение Parakeet…");
        let out = crate::mlx_env::pip_install(
            &venv_python,
            &["--disable-pip-version-check", MLX_PACKAGE],
            "Не удалось запустить pip install parakeet-mlx",
        )?;
        if !out.status.success() {
            return Err(anyhow!(
                "Не удалось установить parakeet-mlx: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        if !crate::mlx_env::python_import_ok(&venv_python, "import parakeet_mlx") {
            return Err(anyhow!("parakeet-mlx не импортируется после установки"));
        }
    }

    on_progress("Скачиваю быструю модель…");
    warmup_mlx_model(app, &venv_python, &on_progress)?;
    on_progress("Готово");
    Ok(())
}

fn warmup_mlx_model(app: &AppHandle, python: &Path, on_progress: &impl Fn(&str)) -> Result<()> {
    let cache_dir = crate::paths::qwen_cache_dir(app)?;
    if mlx_ready_marker_exists(&cache_dir) {
        return Ok(());
    }
    let tmp = crate::paths::tmp_dir(app)?;
    let warmup_wav = tmp.join("parakeet-mlx-warmup.wav");
    crate::transcriber::write_silent_wav(&warmup_wav)?;
    on_progress("Прогреваю модель…");
    let result = transcribe_with_mlx(python, &warmup_wav, &|_| {});
    let _ = std::fs::remove_file(&warmup_wav);
    result?;
    write_mlx_ready_marker(&cache_dir)?;
    Ok(())
}

/// A worker process with the fp16 model resident. One Python interpreter
/// serves every transcription until it idles out or dies; requests are
/// serialized through WORKER like the ONNX model mutex is.
struct MlxWorker {
    child: Child,
    stdin: ChildStdin,
    messages: mpsc::Receiver<WorkerMessage>,
    last_used: Instant,
}
#[derive(Debug, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum WorkerEvent {
    Ready,
    Progress {
        done: u64,
        total: u64,
    },
    Result {
        ok: bool,
        #[serde(default)]
        text: String,
        #[serde(default)]
        error: String,
    },
}

enum WorkerMessage {
    Event(WorkerEvent),
    Eof,
}

/// Generous upper bound per request: MLX RTF on supported Macs stays well
/// below 0.5 even on M1, and the floor covers first-run Metal shader
/// compilation. Hitting it means the worker hung; it gets killed and retried.
fn worker_request_budget(audio_seconds: f64) -> Duration {
    Duration::from_secs_f64((audio_seconds * 0.5 + 120.0).max(150.0))
}

impl MlxWorker {
    fn spawn(python: &Path, hf_home: Option<&Path>) -> Result<Self> {
        let mut command = Command::new(python);
        if let Some(cache) = hf_home {
            command.env("HF_HOME", cache);
        }
        let mut child = command
            .env("HF_HUB_DISABLE_TELEMETRY", "1")
            .env("PYTHONIOENCODING", "utf-8")
            .arg("-c")
            .arg(MLX_WORKER_SCRIPT)
            .arg(MLX_MODEL)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| "failed to spawn parakeet-mlx worker")?;

        let stdin = child.stdin.take().context("worker stdin missing")?;
        let stdout: ChildStdout = child.stdout.take().context("worker stdout missing")?;
        let stderr = child.stderr.take().context("worker stderr missing")?;

        // Surface worker-side tracebacks in the app log instead of dropping them.
        std::thread::spawn(move || {
            let reader = std::io::BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                if !line.trim().is_empty() {
                    tracing::warn!("parakeet-mlx worker: {line}");
                }
            }
        });

        let (tx, rx) = mpsc::channel::<WorkerMessage>();
        std::thread::spawn(move || {
            let mut reader = std::io::BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                match serde_json::from_str::<WorkerEvent>(line.trim()) {
                    Ok(event) => {
                        if tx.send(WorkerMessage::Event(event)).is_err() {
                            break;
                        }
                    }
                    Err(e) => tracing::warn!("parakeet-mlx: ignoring unparsable line: {e}"),
                }
            }
            let _ = tx.send(WorkerMessage::Eof);
        });

        let mut worker = Self {
            child,
            stdin,
            messages: rx,
            last_used: Instant::now(),
        };
        worker.wait_ready()?;
        Ok(worker)
    }

    fn wait_ready(&mut self) -> Result<()> {
        let deadline = Instant::now() + WORKER_READY_TIMEOUT;
        loop {
            match self.next_event(deadline)? {
                WorkerEvent::Ready => return Ok(()),
                WorkerEvent::Progress { .. } => {}
                WorkerEvent::Result { error, .. } => {
                    anyhow::bail!("worker failed before becoming ready: {error}");
                }
            }
        }
    }

    fn next_event(&mut self, deadline: Instant) -> Result<WorkerEvent> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match self.messages.recv_timeout(remaining) {
            Ok(WorkerMessage::Event(event)) => Ok(event),
            Ok(WorkerMessage::Eof) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                self.child.wait().ok();
                anyhow::bail!("parakeet-mlx worker exited unexpectedly")
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                anyhow::bail!("parakeet-mlx worker timed out after {remaining:?}")
            }
        }
    }

    fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    fn request(
        &mut self,
        wav_path: &Path,
        audio_seconds: f64,
        progress_cb: &impl Fn(u32),
    ) -> Result<String> {
        while self.messages.try_recv().is_ok() {
            // Drop events from an aborted previous request, if any.
        }
        self.stdin
            .write_all(format!("{}\n", wav_path.display()).as_bytes())?;
        self.stdin.flush()?;
        self.last_used = Instant::now();

        let deadline = Instant::now() + worker_request_budget(audio_seconds);
        loop {
            match self.next_event(deadline)? {
                WorkerEvent::Ready => {}
                WorkerEvent::Progress { done, total } => {
                    if total > 0 {
                        let pct =
                            ((done as f64 / total as f64) * 90.0 + 5.0).clamp(5.0, 99.0) as u32;
                        progress_cb(pct);
                    }
                }
                WorkerEvent::Result { ok, text, error } => {
                    self.last_used = Instant::now();
                    if ok {
                        return Ok(text);
                    }
                    anyhow::bail!("parakeet-mlx transcription error: {error}");
                }
            }
        }
    }

    fn kill(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

static WORKER: Mutex<Option<MlxWorker>> = Mutex::new(None);

const WORKER_READY_TIMEOUT: Duration = Duration::from_secs(300);
const WORKER_IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

fn transcribe_with_mlx(
    python: &Path,
    wav_path: &Path,
    progress_cb: &impl Fn(u32),
) -> Result<String> {
    progress_cb(5);
    let audio_seconds = hound::WavReader::open(wav_path).map_or(0.0, |r| {
        r.duration() as f64 / f64::from(r.spec().sample_rate)
    });
    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 0..2 {
        let mut guard = WORKER.lock();
        let stale = guard
            .as_mut()
            .map(|w| w.last_used.elapsed() > WORKER_IDLE_TIMEOUT || !w.alive())
            .unwrap_or(false);
        if stale {
            if let Some(dead) = guard.take() {
                dead.kill();
            }
        }
        if guard.is_none() {
            match MlxWorker::spawn(python, mlx_hf_home().as_deref()) {
                Ok(worker) => *guard = Some(worker),
                Err(e) => {
                    if attempt == 0 {
                        tracing::warn!("parakeet-mlx worker spawn failed, retrying once: {e:#}");
                        last_err = Some(e);
                        continue;
                    }
                    return Err(e);
                }
            }
        }
        let result =
            guard
                .as_mut()
                .expect("worker present")
                .request(wav_path, audio_seconds, progress_cb);
        match result {
            Ok(text) => {
                progress_cb(100);
                return Ok(text);
            }
            Err(e) => {
                tracing::warn!(
                    "parakeet-mlx worker request failed ({e:#}); restarting worker and retrying"
                );
                if let Some(dead) = guard.take() {
                    dead.kill();
                }
                last_err = Some(e);
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow!("parakeet-mlx worker failed")))
}

/// Spawn the warm worker now so the next job skips model load entirely.
/// Returns true when a healthy worker is resident.
pub fn preload_worker() -> bool {
    if !crate::hardware::mlx_acceleration_allowed() {
        return false;
    }
    let Some(python) = current_mlx_python() else {
        return false;
    };
    let mut guard = WORKER.lock();
    if let Some(worker) = guard.as_mut() {
        if worker.alive() && worker.last_used.elapsed() <= WORKER_IDLE_TIMEOUT {
            return true;
        }
    }
    match MlxWorker::spawn(&python, mlx_hf_home().as_deref()) {
        Ok(worker) => {
            *guard = Some(worker);
            true
        }
        Err(e) => {
            tracing::warn!("parakeet-mlx worker warmup failed: {e:#}");
            false
        }
    }
}

/// Stop the warm worker and release its ~1.5 GB of memory.
pub fn stop_worker() {
    if let Some(worker) = WORKER.lock().take() {
        worker.kill();
    }
}

pub fn transcribe_wav(
    model_dir: &Path,
    wav_path: &Path,
    progress_cb: impl Fn(u32) + Send + Sync,
) -> Result<String> {
    if crate::hardware::mlx_acceleration_allowed() {
        if let Some(python) = current_mlx_python() {
            match transcribe_with_mlx(&python, wav_path, &progress_cb) {
                Ok(text) if !text.is_empty() => return Ok(text),
                Ok(_) => tracing::warn!("parakeet-mlx returned empty text, falling back to ONNX"),
                Err(e) => tracing::warn!("parakeet-mlx failed, falling back to ONNX: {e:#}"),
            }
        }
    }

    let model = get_or_load_model(model_dir)?;

    let chunk_size = CHUNK_SECONDS * SAMPLE_RATE as usize;
    if let Some(samples) = read_wav_samples_with_limit(wav_path, chunk_size)? {
        progress_cb(5);
        let result = {
            let mut m = model.lock();
            m.transcribe_samples(samples, SAMPLE_RATE, 1, None)
                .map_err(|e| anyhow!("Parakeet transcribe failed: {e:?}"))?
        };
        progress_cb(100);
        return Ok(result.text.trim().to_string());
    }

    transcribe_file_chunks(wav_path, &model, CHUNK_SECONDS, &progress_cb)
}

fn transcribe_file_chunks(
    wav_path: &Path,
    model: &Arc<Mutex<ParakeetTDT>>,
    chunk_seconds: usize,
    progress_cb: &impl Fn(u32),
) -> Result<String> {
    let mut reader = hound::WavReader::open(wav_path)?;
    let spec = reader.spec();
    validate_wav_spec(spec)?;
    let total_samples = reader.duration() as usize;

    match spec.sample_format {
        hound::SampleFormat::Int => {
            let samples = reader
                .samples::<i16>()
                .map(|s| s.map(|v| v as f32 / 32_768.0).map_err(|e| anyhow!(e)));
            transcribe_chunks_from_iter(samples, total_samples, model, chunk_seconds, progress_cb)
        }
        hound::SampleFormat::Float => {
            let samples = reader.samples::<f32>().map(|s| s.map_err(|e| anyhow!(e)));
            transcribe_chunks_from_iter(samples, total_samples, model, chunk_seconds, progress_cb)
        }
    }
}

fn transcribe_chunks_from_iter<I>(
    mut samples: I,
    total_samples: usize,
    model: &Arc<Mutex<ParakeetTDT>>,
    chunk_seconds: usize,
    progress_cb: &impl Fn(u32),
) -> Result<String>
where
    I: Iterator<Item = Result<f32>>,
{
    let chunk_size = chunk_seconds * SAMPLE_RATE as usize;
    let overlap = OVERLAP_SECONDS * SAMPLE_RATE as usize;
    let min_final_samples = MIN_FINAL_CHUNK_SECONDS * SAMPLE_RATE as usize;
    let total_starts = estimated_chunk_count(total_samples, chunk_size, overlap, min_final_samples);
    let mut texts: Vec<String> = Vec::with_capacity(total_starts);
    let mut tail: Vec<f32> = Vec::with_capacity(overlap);
    let mut samples_read = 0usize;

    let mut idx = 0usize;
    loop {
        let mut chunk = Vec::with_capacity(chunk_size);
        if !tail.is_empty() {
            chunk.extend_from_slice(&tail);
        }
        while chunk.len() < chunk_size && samples_read < total_samples {
            let Some(sample) = samples.next() else {
                break;
            };
            chunk.push(sample?);
            samples_read += 1;
        }
        let remaining = total_samples.saturating_sub(samples_read);
        if remaining > 0 && remaining <= min_final_samples {
            while samples_read < total_samples {
                let Some(sample) = samples.next() else {
                    break;
                };
                chunk.push(sample?);
                samples_read += 1;
            }
        }
        if chunk.is_empty() {
            break;
        }

        let next_tail = if samples_read < total_samples {
            let tail_start = chunk.len().saturating_sub(overlap);
            chunk[tail_start..].to_vec()
        } else {
            Vec::new()
        };
        let result = {
            let mut m = model.lock();
            m.transcribe_samples(chunk, SAMPLE_RATE, 1, None)
                .map_err(|e| anyhow!("Parakeet transcribe failed on chunk {idx}: {e:?}"))?
        };
        texts.push(result.text.trim().to_string());
        idx += 1;
        let pct = ((idx as f32 / total_starts.max(1) as f32) * 100.0).min(99.0) as u32;
        progress_cb(pct);
        if samples_read >= total_samples {
            break;
        }
        tail = next_tail;
    }

    progress_cb(100);
    Ok(texts.join(" ").trim().to_string())
}

fn estimated_chunk_count(
    total_samples: usize,
    chunk_size: usize,
    overlap: usize,
    min_final_samples: usize,
) -> usize {
    if total_samples == 0 {
        return 0;
    }
    if total_samples <= chunk_size {
        return 1;
    }

    let stride = chunk_size.saturating_sub(overlap).max(1);
    let mut chunks = 1usize;
    let mut consumed = chunk_size;
    while consumed < total_samples {
        let remaining = total_samples - consumed;
        if remaining <= min_final_samples {
            break;
        }
        chunks += 1;
        consumed = consumed.saturating_add(stride);
    }
    chunks
}

fn read_wav_samples_with_limit(path: &Path, max_samples: usize) -> Result<Option<Vec<f32>>> {
    let reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    validate_wav_spec(spec)?;
    let expected_samples = reader.duration() as usize;
    if expected_samples > max_samples {
        return Ok(None);
    }
    collect_wav_samples(reader, spec, expected_samples).map(Some)
}

#[cfg(test)]
fn read_wav_samples(path: &Path) -> Result<Vec<f32>> {
    let reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    validate_wav_spec(spec)?;
    let expected_samples = reader.duration() as usize;
    collect_wav_samples(reader, spec, expected_samples)
}

fn collect_wav_samples<R: std::io::Read>(
    mut reader: hound::WavReader<R>,
    spec: hound::WavSpec,
    expected_samples: usize,
) -> Result<Vec<f32>> {
    let mut samples = Vec::with_capacity(expected_samples);
    match spec.sample_format {
        hound::SampleFormat::Int => {
            for sample in reader.samples::<i16>() {
                samples.push(sample? as f32 / 32_768.0);
            }
        }
        hound::SampleFormat::Float => {
            for sample in reader.samples::<f32>() {
                samples.push(sample?);
            }
        }
    }
    Ok(samples)
}

fn validate_wav_spec(spec: hound::WavSpec) -> Result<()> {
    if spec.sample_rate != SAMPLE_RATE || spec.channels != 1 {
        return Err(anyhow!(
            "unexpected WAV format: {} Hz, {} channels (need 16000/1)",
            spec.sample_rate,
            spec.channels
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn parakeet_environment_keeps_its_own_runtime_package() {
        assert_eq!(MLX_PACKAGE, "parakeet-mlx==0.5.2");
    }

    #[test]
    fn estimated_chunk_count_should_merge_short_final_audio() {
        let chunk = CHUNK_SECONDS * SAMPLE_RATE as usize;
        let overlap = OVERLAP_SECONDS * SAMPLE_RATE as usize;
        let min_final = MIN_FINAL_CHUNK_SECONDS * SAMPLE_RATE as usize;

        assert_eq!(
            estimated_chunk_count(chunk + SAMPLE_RATE as usize, chunk, overlap, min_final),
            1
        );
        assert_eq!(
            estimated_chunk_count(SAMPLE_RATE as usize * 600, chunk, overlap, min_final),
            2
        );
        assert_eq!(
            estimated_chunk_count(SAMPLE_RATE as usize * 640, chunk, overlap, min_final),
            3
        );
    }

    #[test]
    fn resolve_mlx_python_should_prefer_explicit_file() {
        let python = std::env::temp_dir().join(format!(
            "parrot-mlx-python-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::write(&python, b"#!/bin/sh\n").expect("write fake python");
        let resolved = resolve_mlx_python(Some(python.clone()), &[]);
        let _ = std::fs::remove_file(&python);
        assert_eq!(resolved, Some(python));
    }

    #[test]
    fn resolve_mlx_python_should_find_venv_under_root() {
        let root = std::env::temp_dir().join(format!("parrot-mlx-root-{}", std::process::id()));
        let python = root.join(".qwen-mlx/venv/bin/python");
        std::fs::create_dir_all(python.parent().expect("parent")).expect("mkdir");
        std::fs::write(&python, b"#!/bin/sh\n").expect("write fake python");
        let resolved = resolve_mlx_python(None, std::slice::from_ref(&root));
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(resolved, Some(python));
    }

    #[test]
    fn mlx_ready_marker_should_round_trip() {
        let dir =
            std::env::temp_dir().join(format!("parrot-parakeet-mlx-marker-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");

        assert!(!mlx_ready_marker_exists(&dir));
        write_mlx_ready_marker(&dir).expect("write marker");
        assert!(mlx_ready_marker_exists(&dir));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn worker_events_parse_from_json_lines() {
        let ready: WorkerEvent = serde_json::from_str(r#"{"event":"ready"}"#).expect("parse ready");
        assert!(matches!(ready, WorkerEvent::Ready));

        let progress: WorkerEvent =
            serde_json::from_str(r#"{"event":"progress","done":480000,"total":960000}"#)
                .expect("parse progress");
        assert!(matches!(
            progress,
            WorkerEvent::Progress {
                done: 480000,
                total: 960000
            }
        ));

        let ok: WorkerEvent = serde_json::from_str(
            r#"{"event":"result","ok":true,"text":"привет мир\nвторая строка"}"#,
        )
        .expect("parse result");
        match ok {
            WorkerEvent::Result {
                ok: true,
                text,
                error,
            } => {
                assert_eq!(text, "привет мир\nвторая строка");
                assert!(error.is_empty());
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let err: WorkerEvent =
            serde_json::from_str(r#"{"event":"result","ok":false,"error":"boom"}"#)
                .expect("parse failure result");
        match err {
            WorkerEvent::Result {
                ok: false, error, ..
            } => assert_eq!(error, "boom"),
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn worker_request_budget_scales_with_audio() {
        // Short clips get a floor covering Metal shader compilation.
        assert_eq!(worker_request_budget(0.0), Duration::from_secs_f64(150.0));
        // Long files stay well under the 0.5x RTF ceiling on supported Macs.
        assert_eq!(worker_request_budget(600.0), Duration::from_secs_f64(420.0));
    }

    #[test]
    #[ignore = "manual performance guard: run with `cargo test --release bench_read_wav_samples -- --ignored --nocapture`"]
    fn bench_read_wav_samples() {
        let path = std::env::temp_dir().join(format!(
            "parrot-parakeet-read-bench-{}.wav",
            std::process::id()
        ));
        write_synthetic_wav(&path, SAMPLE_RATE * 180);

        let iterations = std::env::var("PARROT_WAV_READ_BENCH_ITERS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(10);
        let started = Instant::now();
        let mut total_samples = 0usize;
        for _ in 0..iterations {
            let samples = read_wav_samples(&path).expect("read samples");
            total_samples += samples.len();
        }
        let elapsed = started.elapsed();
        let _ = std::fs::remove_file(&path);

        println!(
            "parakeet wav read benchmark: {iterations} iterations, {total_samples} samples, {:.3}s total, {:.3}s/iter",
            elapsed.as_secs_f64(),
            elapsed.as_secs_f64() / iterations as f64
        );
    }

    #[test]
    #[ignore = "manual performance guard: run with `cargo test --release bench_transcribe_mlx_warm -- --ignored --nocapture`"]
    fn bench_transcribe_mlx_warm() {
        let Some(wav) = std::env::var_os("PARROT_TRANSCRIBE_BENCH_WAV") else {
            eprintln!("set PARROT_TRANSCRIBE_BENCH_WAV to run this benchmark");
            return;
        };
        let Some(python) = current_mlx_python() else {
            eprintln!("parakeet-mlx python is not available; skipping benchmark");
            return;
        };
        let wav = std::path::PathBuf::from(wav);
        let audio_seconds = hound::WavReader::open(&wav)
            .map(|r| r.duration() as f64 / f64::from(r.spec().sample_rate))
            .expect("read bench wav header");

        let cold_started = Instant::now();
        let cold = transcribe_with_mlx(&python, &wav, &|_| {}).expect("cold mlx transcription");
        let cold_elapsed = cold_started.elapsed();

        let warm_started = Instant::now();
        let warm = transcribe_with_mlx(&python, &wav, &|_| {}).expect("warm mlx transcription");
        let warm_elapsed = warm_started.elapsed();

        println!(
            "PARROT_BENCH engine=parakeet-mlx audio={audio_seconds:.1}s cold={:.3}s warm={:.3}s warm_rtf={:.2}x chars_cold={} chars_warm={}",
            cold_elapsed.as_secs_f64(),
            warm_elapsed.as_secs_f64(),
            audio_seconds / warm_elapsed.as_secs_f64(),
            cold.len(),
            warm.len(),
        );
    }

    #[test]
    #[ignore = "manual performance guard: run with `cargo test --release bench_transcribe_synthetic_10_min -- --ignored --nocapture`"]
    fn bench_transcribe_synthetic_10_min() {
        let Some(home) = std::env::var_os("HOME") else {
            eprintln!("HOME is not set; skipping benchmark");
            return;
        };
        let model_dir = Path::new(&home)
            .join("Library/Application Support/com.alexk.parrot/models/parakeet-v3");
        if !model_dir.join("encoder-model.int8.onnx").exists() {
            eprintln!("Parakeet model is not installed; skipping benchmark");
            return;
        }

        let env_wav = std::env::var_os("PARROT_TRANSCRIBE_BENCH_WAV").map(std::path::PathBuf::from);
        let (path, duration_seconds, cleanup) = if let Some(path) = env_wav {
            let reader = hound::WavReader::open(&path).expect("open bench wav");
            let duration = reader.duration() as f64 / f64::from(reader.spec().sample_rate);
            (path, duration, false)
        } else {
            let path = std::env::temp_dir().join(format!(
                "parrot-parakeet-transcribe-bench-{}.wav",
                std::process::id()
            ));
            let duration_seconds = std::env::var("PARROT_TRANSCRIBE_BENCH_SECONDS")
                .ok()
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(600.0);
            write_synthetic_wav(&path, (f64::from(SAMPLE_RATE) * duration_seconds) as u32);
            (path, duration_seconds, true)
        };
        preload(&model_dir).expect("preload model");

        let started = Instant::now();
        let text = transcribe_wav(&model_dir, &path, |_| {}).expect("transcribe bench wav");
        let elapsed = started.elapsed();
        if cleanup {
            let _ = std::fs::remove_file(&path);
        }

        let rtf = duration_seconds / elapsed.as_secs_f64();
        if let Some(out) = std::env::var_os("PARROT_TRANSCRIBE_BENCH_OUT") {
            std::fs::write(&out, &text).expect("write bench transcript");
        }
        println!(
            "PARROT_BENCH engine=parakeet-onnx-cpu audio={:.1}s elapsed={:.3}s rtf={:.2}x chars={} label={}",
            duration_seconds,
            elapsed.as_secs_f64(),
            rtf,
            text.len(),
            if cleanup { "synthetic" } else { "file" }
        );
    }

    fn write_synthetic_wav(path: &Path, sample_count: u32) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).expect("create wav");
        for i in 0..sample_count {
            let phase = i as f32 / SAMPLE_RATE as f32 * 440.0 * std::f32::consts::TAU;
            writer
                .write_sample((phase.sin() * i16::MAX as f32 * 0.2) as i16)
                .expect("write sample");
        }
        writer.finalize().expect("finalize wav");
    }
}
