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
    if is_normalized_wav(input) {
        if cancel.as_ref().map(|t| t.is_cancelled()).unwrap_or(false) {
            return Err(anyhow!("cancelled"));
        }
        stage_normalized_wav(input, out_wav)?;
        if cancel.as_ref().map(|t| t.is_cancelled()).unwrap_or(false) {
            let _ = tokio::fs::remove_file(out_wav).await;
            return Err(anyhow!("cancelled"));
        }
        return Ok(out_wav.to_path_buf());
    }

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

fn stage_normalized_wav(input: &Path, out_wav: &Path) -> Result<()> {
    if out_wav.exists() {
        std::fs::remove_file(out_wav)?;
    }
    match std::fs::hard_link(input, out_wav) {
        Ok(()) => Ok(()),
        Err(_) => {
            std::fs::copy(input, out_wav)?;
            Ok(())
        }
    }
}

fn is_normalized_wav(path: &Path) -> bool {
    let Ok(reader) = hound::WavReader::open(path) else {
        return false;
    };
    let spec = reader.spec();
    spec.channels == 1
        && spec.sample_rate == 16_000
        && spec.bits_per_sample == 16
        && spec.sample_format == hound::SampleFormat::Int
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_wav_should_match_transcription_format() {
        let path = temp_wav_path("normalized");
        write_wav(&path, 16_000, 1);

        assert!(is_normalized_wav(&path));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn normalized_wav_should_reject_other_sample_rates() {
        let path = temp_wav_path("rate");
        write_wav(&path, 48_000, 1);

        assert!(!is_normalized_wav(&path));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn normalized_wav_should_reject_stereo() {
        let path = temp_wav_path("stereo");
        write_wav(&path, 16_000, 2);

        assert!(!is_normalized_wav(&path));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn stage_normalized_wav_should_reuse_inode_and_keep_source() {
        let src = temp_wav_path("stage-src");
        let dest = temp_wav_path("stage-dest");
        write_wav(&src, 16_000, 1);

        stage_normalized_wav(&src, &dest).expect("stage wav");

        assert!(dest.exists());
        let src_meta = std::fs::metadata(&src).expect("src meta");
        let dest_meta = std::fs::metadata(&dest).expect("dest meta");
        assert_eq!(src_meta.len(), dest_meta.len());
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(src_meta.ino(), dest_meta.ino());
        }

        std::fs::remove_file(&dest).expect("remove dest");
        assert!(src.exists(), "source wav must survive temp cleanup");
        let _ = std::fs::remove_file(src);
    }

    fn temp_wav_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "parrot-local-{name}-{}-{}.wav",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ))
    }

    fn write_wav(path: &Path, sample_rate: u32, channels: u16) {
        let spec = hound::WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).expect("create wav");
        for _ in 0..sample_rate {
            for _ in 0..channels {
                writer.write_sample::<i16>(0).expect("write sample");
            }
        }
        writer.finalize().expect("finalize wav");
    }
}
