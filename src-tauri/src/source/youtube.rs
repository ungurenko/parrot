use anyhow::{anyhow, Result};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tauri::AppHandle;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;

use crate::binaries;
use crate::cancellation::CancelToken;

const FAST_YOUTUBE_ARGS: &str =
    "youtube:player_client=android_vr;player_skip=js;skip=hls,dash,translated_subs";

pub struct YouTubeAudio {
    pub wav_path: PathBuf,
    pub title: String,
}

/// Download raw audio from a YouTube URL (m4a/opus/webm) via yt-dlp, then convert to
/// 16 kHz mono PCM16 WAV via ffmpeg in a single pass.
pub async fn download_wav(
    app: &AppHandle,
    url: &str,
    workdir: &Path,
    stem: &str,
    cancel: Option<Arc<CancelToken>>,
    on_progress: impl Fn(u32) + Send + Sync + 'static,
) -> Result<YouTubeAudio> {
    let yt_dlp = binaries::yt_dlp_path(app)?;
    let ffmpeg = binaries::ffmpeg_path(app)?;

    let (raw_audio, title) = match download_raw_audio(
        &yt_dlp,
        url,
        workdir,
        stem,
        Some(FAST_YOUTUBE_ARGS),
        cancel.as_ref(),
        &on_progress,
    )
    .await
    {
        Ok(result) => result,
        Err(fast_err) => {
            if cancel.as_ref().map(|t| t.is_cancelled()).unwrap_or(false) {
                return Err(fast_err);
            }
            tracing::warn!(
                "fast YouTube extraction failed, retrying with default yt-dlp path: {fast_err:#}"
            );
            cleanup_partial_downloads(workdir, stem);
            on_progress(1);
            download_raw_audio(&yt_dlp, url, workdir, stem, None, cancel.as_ref(), &on_progress)
                .await?
        }
    };
    on_progress(100);

    let final_wav = normalize_audio(&ffmpeg, &raw_audio, workdir, stem, cancel.as_ref()).await?;
    let _ = std::fs::remove_file(&raw_audio);

    Ok(YouTubeAudio {
        wav_path: final_wav,
        title: title.unwrap_or_else(|| stem.to_string()),
    })
}

async fn download_raw_audio(
    yt_dlp: &Path,
    url: &str,
    workdir: &Path,
    stem: &str,
    extractor_args: Option<&str>,
    cancel: Option<&Arc<CancelToken>>,
    on_progress: &(impl Fn(u32) + Send + Sync),
) -> Result<(PathBuf, Option<String>)> {
    // Download raw bestaudio without re-muxing — much smaller and faster.
    let template = workdir.join(format!("{stem}.%(ext)s"));
    let mut command = Command::new(yt_dlp);
    binaries::apply_yt_dlp_startup_cache(&mut command);
    command
        .arg("--ignore-config")
        .arg("-f")
        .arg("bestaudio/best")
        .arg("-N")
        .arg("4")
        .arg("--no-playlist")
        .arg("--newline")
        .arg("--color")
        .arg("stdout:never")
        .arg("--print")
        .arg("before_dl:TITLE %(title)s")
        .arg("--progress-template")
        .arg("download:PROGRESS %(progress._percent_str)s")
        .arg("-o")
        .arg(&template);
    if let Some(args) = extractor_args {
        command.arg("--extractor-args").arg(args);
    }
    let mut child = command
        .arg(url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let pid = child.id();
    if let (Some(tok), Some(pid)) = (cancel, pid) {
        tok.register_pid(pid);
    }

    let stderr_task = child.stderr.take().map(|stderr| {
        tauri::async_runtime::spawn(async move { collect_tail(stderr, 20).await })
    });

    let mut title: Option<String> = None;
    if let Some(stdout) = child.stdout.take() {
        let mut lines = BufReader::new(stdout).lines();
        while let Some(line) = lines.next_line().await? {
            if let Some(p) = parse_progress(&line) {
                on_progress(p);
            } else if let Some(t) = parse_title(&line) {
                title = Some(t);
            }
        }
    }

    let status = child.wait().await?;
    if let (Some(tok), Some(pid)) = (cancel, pid) {
        tok.unregister_pid(pid);
    }
    let stderr_tail = match stderr_task {
        Some(task) => task.await.unwrap_or_default(),
        None => Vec::new(),
    };
    if !status.success() {
        if cancel.map(|t| t.is_cancelled()).unwrap_or(false) {
            return Err(anyhow!("cancelled"));
        }
        return Err(anyhow!(
            "yt-dlp failed with status {:?}: {}",
            status.code(),
            stderr_tail.join("\n")
        ));
    }

    // Find the downloaded file (the extension depends on the chosen stream).
    let raw_audio = find_downloaded(workdir, stem)?;
    Ok((raw_audio, title))
}

async fn normalize_audio(
    ffmpeg: &Path,
    raw_audio: &Path,
    workdir: &Path,
    stem: &str,
    cancel: Option<&Arc<CancelToken>>,
) -> Result<PathBuf> {
    let final_wav = workdir.join(format!("{stem}.wav"));
    let child = Command::new(ffmpeg)
        .arg("-y")
        .arg("-nostdin")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-i")
        .arg(&raw_audio)
        .arg("-vn")
        .arg("-ac")
        .arg("1")
        .arg("-ar")
        .arg("16000")
        .arg("-c:a")
        .arg("pcm_s16le")
        .arg(&final_wav)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    let pid = child.id();
    if let (Some(tok), Some(pid)) = (cancel, pid) {
        tok.register_pid(pid);
    }
    let norm_output = child.wait_with_output().await?;
    if let (Some(tok), Some(pid)) = (cancel, pid) {
        tok.unregister_pid(pid);
    }
    if !norm_output.status.success() {
        if cancel.map(|t| t.is_cancelled()).unwrap_or(false) {
            return Err(anyhow!("cancelled"));
        }
        return Err(anyhow!(
            "ffmpeg conversion failed: {}",
            String::from_utf8_lossy(&norm_output.stderr).trim()
        ));
    }
    Ok(final_wav)
}

fn find_downloaded(workdir: &Path, stem: &str) -> Result<PathBuf> {
    for entry in std::fs::read_dir(workdir)? {
        let entry = entry?;
        let path = entry.path();
        let file_stem = path.file_stem().and_then(|s| s.to_str());
        if file_stem == Some(stem) {
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            // Skip our own final wav if it somehow exists.
            if ext != "wav" {
                return Ok(path);
            }
        }
    }
    Err(anyhow!("downloaded audio not found for stem {stem}"))
}

fn cleanup_partial_downloads(workdir: &Path, stem: &str) {
    if let Ok(entries) = std::fs::read_dir(workdir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if file_name.starts_with(&format!("{stem}.")) {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

fn parse_progress(line: &str) -> Option<u32> {
    let rest = line.strip_prefix("PROGRESS")?.trim();
    let num_str = rest.trim_end_matches('%').trim();
    let v: f32 = num_str.parse().ok()?;
    Some(v.clamp(0.0, 100.0) as u32)
}

fn parse_title(line: &str) -> Option<String> {
    let title = line.strip_prefix("TITLE ")?.trim();
    if title.is_empty() {
        None
    } else {
        Some(title.to_string())
    }
}

async fn collect_tail<R>(reader: R, max_lines: usize) -> Vec<String>
where
    R: AsyncRead + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    let mut tail = VecDeque::with_capacity(max_lines);
    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        if tail.len() == max_lines {
            tail.pop_front();
        }
        tail.push_back(line);
    }
    tail.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_progress_should_read_yt_dlp_percent() {
        assert_eq!(parse_progress("PROGRESS 42.7%"), Some(42));
    }

    #[test]
    fn parse_title_should_read_printed_title() {
        assert_eq!(
            parse_title("TITLE Long lecture"),
            Some("Long lecture".to_string())
        );
    }
}
