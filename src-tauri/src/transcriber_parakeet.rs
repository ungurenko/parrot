use anyhow::{anyhow, Context, Result};
use once_cell::sync::Lazy;
use parakeet_rs::{
    ExecutionConfig as ParakeetExecConfig, ExecutionProvider, ParakeetTDT, Transcriber,
};
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

// Parakeet TDT has a ~8-10 min sequence-length limit. Five-minute chunks are
// the stable path for long lectures and interviews.
const CHUNK_SECONDS: usize = 5 * 60;
const OVERLAP_SECONDS: usize = 5;
const MIN_FINAL_CHUNK_SECONDS: usize = 30;
const SAMPLE_RATE: u32 = 16_000;

static MODEL: Lazy<Mutex<Option<Arc<Mutex<ParakeetTDT>>>>> = Lazy::new(|| Mutex::new(None));
static MLX_PYTHON: Mutex<Option<Option<PathBuf>>> = Mutex::new(None);
static MLX_INSTALL_STARTED: AtomicBool = AtomicBool::new(false);

const MLX_MODEL: &str = "mlx-community/parakeet-tdt-0.6b-v3";
const MLX_READY_MARKER: &str = ".parrot-ready-parakeet-mlx";
const MLX_PACKAGE: &str = "parakeet-mlx==0.5.2";

const MLX_TRANSCRIBE_SCRIPT: &str = r#"
import sys
from parakeet_mlx import from_pretrained
model = from_pretrained(sys.argv[1])
print(model.transcribe(sys.argv[2], chunk_duration=120.0, overlap_duration=15.0).text)
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

pub fn is_mlx_ready() -> bool {
    current_mlx_python().is_some()
}

/// Install Python + parakeet-mlx + download weights in the background.
/// Transcription keeps using ONNX until this finishes.
pub fn spawn_mlx_install(app: AppHandle) {
    if is_mlx_ready() {
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
    .filter(|python| mlx_python_has_package(python))
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

fn mlx_python_has_package(python: &Path) -> bool {
    Command::new(python)
        .args(["-c", "import parakeet_mlx"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
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
    let venv_python = crate::mlx_env::ensure_user_python_venv(app, &on_progress)?;
    if mlx_python_has_package(&venv_python) {
        if let Ok(cache) = crate::paths::qwen_cache_dir(app) {
            if mlx_ready_marker_exists(&cache) {
                on_progress("Ускорение уже готово");
                return Ok(());
            }
        }
    } else {
        on_progress("Устанавливаю ускорение Parakeet…");
        let out = Command::new(&venv_python)
            .args([
                "-m",
                "pip",
                "install",
                "--disable-pip-version-check",
                MLX_PACKAGE,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .context("Не удалось запустить pip install parakeet-mlx")?;
        if !out.status.success() {
            return Err(anyhow!(
                "Не удалось установить parakeet-mlx: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        if !mlx_python_has_package(&venv_python) {
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
    write_silent_wav(&warmup_wav)?;
    on_progress("Прогреваю модель…");
    let result = transcribe_with_mlx(python, &warmup_wav, &|_| {});
    let _ = std::fs::remove_file(&warmup_wav);
    result?;
    write_mlx_ready_marker(&cache_dir)?;
    Ok(())
}

fn write_silent_wav(path: &Path) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for _ in 0..SAMPLE_RATE {
        writer.write_sample::<i16>(0)?;
    }
    writer.finalize()?;
    Ok(())
}

fn transcribe_with_mlx(
    python: &Path,
    wav_path: &Path,
    progress_cb: &impl Fn(u32),
) -> Result<String> {
    progress_cb(5);
    let mut command = Command::new(python);
    if let Some(cache) = mlx_hf_home() {
        command.env("HF_HOME", cache);
    }
    let output = command
        .env("HF_HUB_DISABLE_TELEMETRY", "1")
        .arg("-c")
        .arg(MLX_TRANSCRIBE_SCRIPT)
        .arg(MLX_MODEL)
        .arg(wav_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;
    if !output.status.success() {
        let tail = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "parakeet-mlx failed: {}",
            tail.trim()
                .lines()
                .rev()
                .take(8)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    progress_cb(100);
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn transcribe_wav(
    model_dir: &Path,
    wav_path: &Path,
    progress_cb: impl Fn(u32) + Send + Sync,
) -> Result<String> {
    if let Some(python) = current_mlx_python() {
        match transcribe_with_mlx(&python, wav_path, &progress_cb) {
            Ok(text) if !text.is_empty() => return Ok(text),
            Ok(_) => tracing::warn!("parakeet-mlx returned empty text, falling back to ONNX"),
            Err(e) => tracing::warn!("parakeet-mlx failed, falling back to ONNX: {e:#}"),
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
