use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use parakeet_rs::{ExecutionConfig as ParakeetExecConfig, ExecutionProvider, ParakeetTDT, Transcriber};
use parking_lot::Mutex;
use std::path::Path;
use std::sync::Arc;

// Parakeet TDT has a ~8-10 min sequence-length limit. Five-minute chunks are
// the stable path for long lectures and interviews.
const CHUNK_SECONDS: usize = 5 * 60;
const OVERLAP_SECONDS: usize = 5;
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
    let total_samples = wav_sample_count(wav_path)?;

    if total_samples <= chunk_size {
        let samples = read_wav_samples(wav_path)?;
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
    // Chunked path: stride = chunk_size - overlap so adjacent chunks share tail/head context.
    let stride = chunk_size - overlap;
    let total_starts: usize = (total_samples - overlap + stride - 1) / stride;
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

fn read_wav_samples(path: &Path) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    validate_wav_spec(spec)?;
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => reader
            .samples::<i16>()
            .map(|s| s.map(|v| v as f32 / 32_768.0))
            .collect::<Result<Vec<_>, _>>()?,
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<Vec<_>, _>>()?,
    };
    Ok(samples)
}

fn wav_sample_count(path: &Path) -> Result<usize> {
    let reader = hound::WavReader::open(path)?;
    validate_wav_spec(reader.spec())?;
    Ok(reader.duration() as usize)
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
