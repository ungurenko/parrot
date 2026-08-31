use std::path::PathBuf;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};

use crate::fs_metrics::dir_size_bytes;

pub(crate) mod history;
pub(crate) mod jobs;
pub(crate) mod models;
pub(crate) mod summary;
pub(crate) mod system;
pub(crate) mod translation;

pub(crate) fn spawn_model_progress_poller(
    app: AppHandle,
    cache_dir: PathBuf,
    expected_bytes: u64,
    progress_event: &'static str,
    stage_event: &'static str,
) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        let warmup_threshold = (expected_bytes as f64 * 0.9) as u64;
        let mut warmup_started: Option<Instant> = None;
        let mut warmup_emitted = false;
        loop {
            tokio::time::sleep(Duration::from_millis(1500)).await;
            let size = dir_size_bytes(&cache_dir);
            if size >= warmup_threshold {
                let started = warmup_started.get_or_insert_with(Instant::now);
                if !warmup_emitted {
                    let _ = app.emit(stage_event, "warmup");
                    warmup_emitted = true;
                }
                let _ = app.emit(progress_event, warmup_percent(started.elapsed()));
            } else {
                let _ = app.emit(progress_event, download_percent(size, expected_bytes));
            }
        }
    })
}

pub(crate) async fn run_model_warmup(
    app: AppHandle,
    cache_dir: PathBuf,
    expected_bytes: u64,
    progress_event: &'static str,
    stage_event: &'static str,
    warmup: impl FnOnce() -> anyhow::Result<()> + Send + 'static,
) -> Result<(), String> {
    let _ = app.emit(progress_event, 1u32);
    let _ = app.emit(stage_event, "downloading");
    let poll_handle = spawn_model_progress_poller(
        app.clone(),
        cache_dir,
        expected_bytes,
        progress_event,
        stage_event,
    );
    let result = tauri::async_runtime::spawn_blocking(warmup).await;
    poll_handle.abort();
    result
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let _ = app.emit(stage_event, "ready");
    let _ = app.emit(progress_event, 100u32);
    Ok(())
}

fn download_percent(size: u64, expected_bytes: u64) -> u32 {
    ((size as f64 / expected_bytes as f64) * 95.0).clamp(1.0, 95.0) as u32
}

fn warmup_percent(elapsed: Duration) -> u32 {
    (95.0 + (elapsed.as_secs_f64() / 20.0).min(1.0) * 4.0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_progress_stays_inside_reserved_ranges() {
        assert_eq!(download_percent(0, 100), 1);
        assert_eq!(download_percent(50, 100), 47);
        assert_eq!(download_percent(100, 100), 95);
        assert_eq!(warmup_percent(Duration::ZERO), 95);
        assert_eq!(warmup_percent(Duration::from_secs(10)), 97);
        assert_eq!(warmup_percent(Duration::from_secs(30)), 99);
    }
}
