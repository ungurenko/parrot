use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tauri::AppHandle;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

use crate::binaries;
use crate::cancellation::CancelToken;

const FFMPEG_TIMEOUT: Duration = Duration::from_secs(60 * 60);

/// Extract a 16 kHz mono PCM16 WAV from any audio or video file using bundled ffmpeg.
pub async fn extract_wav(
    app: &AppHandle,
    input: &Path,
    out_wav: &Path,
    cancel: Option<Arc<CancelToken>>,
) -> Result<PathBuf> {
    let ffmpeg = binaries::ffmpeg_path(app)?;
    let child = Command::new(&ffmpeg)
        .arg("-y")
        .arg("-nostdin")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-i")
        .arg(input)
        .arg("-vn")
        .arg("-ac")
        .arg("1")
        .arg("-ar")
        .arg("16000")
        .arg("-c:a")
        .arg("pcm_s16le")
        .arg(out_wav)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    let pid = child.id();
    if let (Some(tok), Some(pid)) = (cancel.as_ref(), pid) {
        tok.register_pid(pid);
    }
    let output = match timeout(FFMPEG_TIMEOUT, child.wait_with_output()).await {
        Ok(output) => output?,
        Err(_) => {
            if let (Some(tok), Some(pid)) = (cancel.as_ref(), pid) {
                tok.unregister_pid(pid);
            }
            return Err(anyhow!("ffmpeg не ответил за отведенное время"));
        }
    };
    if let (Some(tok), Some(pid)) = (cancel.as_ref(), pid) {
        tok.unregister_pid(pid);
    }
    if !output.status.success() {
        if cancel.as_ref().map(|t| t.is_cancelled()).unwrap_or(false) {
            return Err(anyhow!("cancelled"));
        }
        return Err(anyhow!(
            "ffmpeg failed with status {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(out_wav.to_path_buf())
}
