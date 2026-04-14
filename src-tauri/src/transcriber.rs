use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::path::Path;
use std::sync::Arc;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

// Cache the loaded WhisperContext so we don't re-parse the ~1.5 GB model for every job.
static CONTEXT: Lazy<Mutex<Option<Arc<WhisperContext>>>> = Lazy::new(|| Mutex::new(None));

/// Load the WhisperContext now (blocking) so the first transcription doesn't pay the
/// ~1.5 GB model load and the Core ML first-run compile up-front.
pub fn preload(model_path: &Path) -> Result<()> {
    get_or_load_context(model_path)?;
    Ok(())
}

fn get_or_load_context(model_path: &Path) -> Result<Arc<WhisperContext>> {
    let mut guard = CONTEXT.lock();
    if guard.is_none() {
        let params = WhisperContextParameters::default();
        let ctx = WhisperContext::new_with_params(
            model_path
                .to_str()
                .ok_or_else(|| anyhow!("invalid model path"))?,
            params,
        )?;
        *guard = Some(Arc::new(ctx));
    }
    Ok(guard.as_ref().unwrap().clone())
}

pub fn transcribe_wav(
    model_path: &Path,
    wav_path: &Path,
    progress_cb: impl Fn(u32) + Send + Sync + 'static,
) -> Result<String> {
    let samples = read_wav_samples(wav_path)?;
    let ctx = get_or_load_context(model_path)?;
    let mut state = ctx.create_state()?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_n_threads(preferred_threads() as i32);
    params.set_language(Some("auto"));
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_suppress_blank(true);
    params.set_progress_callback_safe(move |p| progress_cb(p as u32));

    state.full(params, &samples)?;

    let mut out = String::new();
    for segment in state.as_iter() {
        let text = segment.to_str_lossy()?.to_string();
        out.push_str(text.trim());
        out.push('\n');
    }
    Ok(out.trim().to_string())
}

fn read_wav_samples(path: &Path) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    if spec.sample_rate != 16_000 || spec.channels != 1 {
        return Err(anyhow!(
            "unexpected WAV format: {} Hz, {} channels (need 16000/1)",
            spec.sample_rate,
            spec.channels
        ));
    }
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => reader
            .samples::<i16>()
            .map(|s| s.map(|v| v as f32 / 32_768.0))
            .collect::<Result<Vec<_>, _>>()?,
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()?,
    };
    Ok(samples)
}

fn preferred_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get().min(8))
        .unwrap_or(4)
}
