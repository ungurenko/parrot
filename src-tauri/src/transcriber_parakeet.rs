use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use parakeet_rs::{
    ExecutionConfig as ParakeetExecConfig, ExecutionProvider, ParakeetTDT, Transcriber,
};
use parking_lot::Mutex;
use std::path::Path;
use std::sync::Arc;

// Parakeet TDT has a ~8-10 min sequence-length limit. Five-minute chunks are
// the stable path for long lectures and interviews.
const CHUNK_SECONDS: usize = 5 * 60;
const OVERLAP_SECONDS: usize = 5;
const MIN_FINAL_CHUNK_SECONDS: usize = 30;
const SAMPLE_RATE: u32 = 16_000;

static MODEL: Lazy<Mutex<Option<Arc<Mutex<ParakeetTDT>>>>> = Lazy::new(|| Mutex::new(None));

pub fn preload(model_dir: &Path) -> Result<()> {
    get_or_load_model(model_dir)?;
    Ok(())
}

pub fn clear_cache() {
    *MODEL.lock() = None;
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

pub fn transcribe_wav(
    model_dir: &Path,
    wav_path: &Path,
    progress_cb: impl Fn(u32) + Send + Sync,
) -> Result<String> {
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

        let path = std::env::temp_dir().join(format!(
            "parrot-parakeet-transcribe-bench-{}.wav",
            std::process::id()
        ));
        let duration_seconds = std::env::var("PARROT_TRANSCRIBE_BENCH_SECONDS")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(600);
        write_synthetic_wav(&path, SAMPLE_RATE * duration_seconds);
        preload(&model_dir).expect("preload model");

        let started = Instant::now();
        let text = transcribe_wav(&model_dir, &path, |_| {}).expect("transcribe synthetic wav");
        let elapsed = started.elapsed();
        let _ = std::fs::remove_file(&path);

        println!(
            "parakeet synthetic {duration_seconds}s benchmark: {:.3}s, text chars {}",
            elapsed.as_secs_f64(),
            text.len()
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
