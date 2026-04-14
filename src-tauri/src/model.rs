use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use std::path::Path;
use std::process::Stdio;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

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
    let resp = client.get(url).send().await?.error_for_status()?;
    let total = resp.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(&tmp).await?;
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    let span = (percent_end - percent_start) as f64;
    let mut last_emit: u32 = percent_start;
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| anyhow!(e))?;
        file.write_all(&bytes).await?;
        downloaded = downloaded.saturating_add(bytes.len() as u64);
        if total > 0 {
            let frac = downloaded as f64 / total as f64;
            let pct = percent_start + (frac * span).round() as u32;
            if pct != last_emit {
                last_emit = pct;
                let _ = app.emit("model:progress", pct);
            }
        }
    }
    file.flush().await?;
    drop(file);
    tokio::fs::rename(&tmp, dest).await?;
    let _ = app.emit("model:progress", percent_end);
    Ok(())
}
