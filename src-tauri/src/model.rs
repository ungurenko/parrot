use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use reqwest::header::{CONTENT_RANGE, RANGE};
use reqwest::StatusCode;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use serde::Serialize;

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

// Parakeet v3 ONNX model URLs (istupakov/parakeet-tdt-0.6b-v3-onnx)
const PARAKEET_BASE: &str =
    "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main";

pub async fn download_whisper(
    app: AppHandle,
    main_dest: &Path,
    coreml_dest: &Path,
) -> Result<()> {
    if let Some(parent) = main_dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    if !main_dest.exists() {
        download_with_progress(&app, WHISPER_MAIN_URL, main_dest, 0, 80).await?;
    } else {
        let _ = app.emit("model:progress", 80u32);
    }

    if !coreml_dest.exists() {
        let models_dir = coreml_dest
            .parent()
            .ok_or_else(|| anyhow!("no parent for coreml path"))?;
        let zip_path = models_dir.join("coreml-encoder.zip");
        download_with_progress(&app, WHISPER_COREML_URL, &zip_path, 80, 98).await?;

        let status = Command::new("unzip")
            .arg("-o")
            .arg("-q")
            .arg(&zip_path)
            .arg("-d")
            .arg(models_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?
            .wait()
            .await?;
        if !status.success() {
            return Err(anyhow!("unzip failed with status {:?}", status.code()));
        }
        let _ = tokio::fs::remove_file(&zip_path).await;
    }

    let _ = app.emit("model:progress", 100u32);
    Ok(())
}

pub async fn download_parakeet(app: AppHandle, dir: &Path) -> Result<()> {
    tokio::fs::create_dir_all(dir).await?;

    // Download int8 variant — ~3× smaller than fp32 and ~2× faster inference.
    // parakeet-rs auto-discovers these filenames alongside the fp32 variant.
    let files: [(&str, u32, u32); 3] = [
        ("vocab.txt", 0, 1),
        ("decoder_joint-model.int8.onnx", 1, 5),
        ("encoder-model.int8.onnx", 5, 100),
    ];

    for (name, start, end) in files {
        let dest = dir.join(name);
        if dest.exists() {
            let _ = app.emit("model:progress", end);
            continue;
        }
        let url = format!("{PARAKEET_BASE}/{name}");
        download_with_progress(&app, &url, &dest, start, end).await?;
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
) -> Result<()> {
    let tmp = dest.with_extension("part");
    let client = reqwest::Client::builder()
        .user_agent("parrot/0.1")
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

    emit_download_progress(
        app,
        percent_start,
        downloaded,
        total,
        0,
    );

    while let Some(chunk) = stream.next().await {
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
