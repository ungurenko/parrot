use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use reqwest::header::{CONTENT_RANGE, RANGE};
use reqwest::StatusCode;
use serde::Serialize;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::cancellation::CancelToken;

#[derive(Clone, Serialize)]
struct ModelProgressDetail {
    percent: u32,
    downloaded_bytes: u64,
    total_bytes: u64,
    speed_bytes_per_sec: u64,
}

// Whisper model URLs
const WHISPER_MAIN_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin";
const WHISPER_COREML_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-encoder.mlmodelc.zip";
const WHISPER_MAIN_MIN_BYTES: u64 = 100 * 1024 * 1024;

// Parakeet v3 ONNX model URLs (istupakov/parakeet-tdt-0.6b-v3-onnx)
const PARAKEET_BASE: &str =
    "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main";
const DOWNLOAD_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const DOWNLOAD_READ_TIMEOUT: Duration = Duration::from_secs(60);

pub async fn download_whisper(
    app: AppHandle,
    main_dest: &Path,
    coreml_dest: &Path,
    cancel: &CancelToken,
) -> Result<()> {
    if let Some(parent) = main_dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    if !file_len_at_least(main_dest, WHISPER_MAIN_MIN_BYTES) {
        let _ = tokio::fs::remove_file(main_dest).await;
        download_with_progress(&app, WHISPER_MAIN_URL, main_dest, 0, 80, cancel).await?;
    } else {
        let _ = app.emit("model:progress", 80u32);
    }

    ensure_not_cancelled(cancel)?;
    if !dir_has_content(coreml_dest) {
        let _ = tokio::fs::remove_dir_all(coreml_dest).await;
        let models_dir = coreml_dest
            .parent()
            .ok_or_else(|| anyhow!("no parent for coreml path"))?;
        let zip_path = models_dir.join("coreml-encoder.zip");
        download_with_progress(&app, WHISPER_COREML_URL, &zip_path, 80, 98, cancel).await?;

        ensure_not_cancelled(cancel)?;
        let mut child = Command::new("unzip")
            .arg("-o")
            .arg("-q")
            .arg(&zip_path)
            .arg("-d")
            .arg(models_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;
        let pid = child.id();
        if let Some(pid) = pid {
            cancel.register_pid(pid);
        }
        let status = child.wait().await?;
        if let Some(pid) = pid {
            cancel.unregister_pid(pid);
        }
        if !status.success() {
            ensure_not_cancelled(cancel)?;
            return Err(anyhow!("unzip failed with status {:?}", status.code()));
        }
        let _ = tokio::fs::remove_file(&zip_path).await;
    }

    let _ = app.emit("model:progress", 100u32);
    Ok(())
}

pub async fn download_parakeet(app: AppHandle, dir: &Path, cancel: &CancelToken) -> Result<()> {
    tokio::fs::create_dir_all(dir).await?;

    // Download int8 variant — ~3× smaller than fp32 and ~2× faster inference.
    // parakeet-rs auto-discovers these filenames alongside the fp32 variant.
    let files: [(&str, u64, u32, u32); 3] = [
        ("vocab.txt", 100, 0, 1),
        ("decoder_joint-model.int8.onnx", 1024 * 1024, 1, 5),
        ("encoder-model.int8.onnx", 100 * 1024 * 1024, 5, 100),
    ];

    for (name, min_bytes, start, end) in files {
        ensure_not_cancelled(cancel)?;
        let dest = dir.join(name);
        if file_len_at_least(&dest, min_bytes) {
            let _ = app.emit("model:progress", end);
            continue;
        }
        let _ = tokio::fs::remove_file(&dest).await;
        let url = format!("{PARAKEET_BASE}/{name}");
        download_with_progress(&app, &url, &dest, start, end, cancel).await?;
    }

    let _ = app.emit("model:progress", 100u32);
    Ok(())
}

async fn download_with_progress(
    app: &AppHandle,
    url: &str,
    dest: &Path,
    percent_start: u32,
    percent_end: u32,
    cancel: &CancelToken,
) -> Result<()> {
    ensure_not_cancelled(cancel)?;
    let tmp = dest.with_extension("part");
    let client = reqwest::Client::builder()
        .user_agent("parrot/0.1")
        .connect_timeout(DOWNLOAD_CONNECT_TIMEOUT)
        .read_timeout(DOWNLOAD_READ_TIMEOUT)
        .build()?;
    let resume_from = tokio::fs::metadata(&tmp)
        .await
        .map(|meta| meta.len())
        .unwrap_or(0);
    let mut request = client.get(url);
    if resume_from > 0 {
        request = request.header(RANGE, format!("bytes={resume_from}-"));
    }

    let resp = request.send().await?.error_for_status()?;
    let status = resp.status();
    let content_length = resp.content_length().unwrap_or(0);
    let content_range = resp
        .headers()
        .get(CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_content_range_total);
    let can_resume = resume_from > 0 && status == StatusCode::PARTIAL_CONTENT;
    let mut downloaded = if can_resume { resume_from } else { 0 };
    let total = if can_resume {
        content_range.unwrap_or_else(|| resume_from.saturating_add(content_length))
    } else {
        content_length
    };

    let mut file = if can_resume {
        tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&tmp)
            .await?
    } else {
        tokio::fs::File::create(&tmp).await?
    };
    let mut stream = resp.bytes_stream();
    let span = (percent_end - percent_start) as f64;
    let mut last_emit: u32 = percent_start;
    let started = Instant::now();
    let mut session_downloaded: u64 = 0;
    let mut last_detail_emit = Instant::now()
        .checked_sub(Duration::from_secs(2))
        .unwrap_or_else(Instant::now);

    emit_download_progress(app, percent_start, downloaded, total, 0);

    while let Some(chunk) = stream.next().await {
        ensure_not_cancelled(cancel)?;
        let bytes = chunk.map_err(|e| anyhow!(e))?;
        file.write_all(&bytes).await?;
        downloaded = downloaded.saturating_add(bytes.len() as u64);
        session_downloaded = session_downloaded.saturating_add(bytes.len() as u64);
        if total > 0 {
            let frac = downloaded as f64 / total as f64;
            let pct = (percent_start + (frac * span).round() as u32).min(percent_end);
            let elapsed = started.elapsed().as_secs_f64().max(0.001);
            let speed = (session_downloaded as f64 / elapsed).round() as u64;
            let should_emit_detail = last_detail_emit.elapsed() >= Duration::from_secs(1);
            if pct != last_emit || should_emit_detail {
                last_emit = pct;
                last_detail_emit = Instant::now();
                emit_download_progress(app, pct, downloaded, total, speed);
            }
        }
    }
    file.flush().await?;
    drop(file);
    tokio::fs::rename(&tmp, dest).await?;
    emit_download_progress(app, percent_end, total.max(downloaded), total, 0);
    Ok(())
}

fn ensure_not_cancelled(cancel: &CancelToken) -> Result<()> {
    if cancel.is_cancelled() {
        anyhow::bail!("cancelled");
    }
    Ok(())
}

fn emit_download_progress(
    app: &AppHandle,
    percent: u32,
    downloaded_bytes: u64,
    total_bytes: u64,
    speed_bytes_per_sec: u64,
) {
    let _ = app.emit("model:progress", percent);
    let _ = app.emit(
        "model:progress_detail",
        ModelProgressDetail {
            percent,
            downloaded_bytes,
            total_bytes,
            speed_bytes_per_sec,
        },
    );
}

fn parse_content_range_total(value: &str) -> Option<u64> {
    value.rsplit('/').next()?.parse().ok()
}

fn file_len_at_least(path: &Path, min_bytes: u64) -> bool {
    path.metadata()
        .map(|meta| meta.is_file() && meta.len() >= min_bytes)
        .unwrap_or(false)
}

fn dir_has_content(path: &Path) -> bool {
    path.is_dir()
        && std::fs::read_dir(path)
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("parrot-model-test-{}-{}", name, std::process::id()))
    }

    #[test]
    fn file_len_at_least_should_reject_empty_files() {
        let path = temp_path("empty-file");
        let _ = std::fs::remove_file(&path);
        std::fs::write(&path, []).expect("write empty file");

        assert!(!file_len_at_least(&path, 1));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn dir_has_content_should_reject_empty_dirs() {
        let path = temp_path("empty-dir");
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create temp dir");

        assert!(!dir_has_content(&path));

        let _ = std::fs::remove_dir_all(&path);
    }
}
