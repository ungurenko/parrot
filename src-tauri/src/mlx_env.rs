use anyhow::{anyhow, Context, Result};
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use tar::Archive;
use tauri::AppHandle;

use crate::paths;

// Pinned standalone Python release from astral-sh/python-build-standalone.
// Update the URL and SHA256 together using SHA256SUMS from the same release.
const STANDALONE_PYTHON_URL: &str = "https://github.com/astral-sh/python-build-standalone/releases/download/20260414/cpython-3.12.13+20260414-aarch64-apple-darwin-install_only.tar.gz";
const STANDALONE_PYTHON_SHA256: &str =
    "8966b2bcd9fa03ba22c080ad15a86bc12e41a00122b16f4b3740e302261124d9";
const STANDALONE_PYTHON_BYTES: u64 = 17_836_558;
const DOWNLOAD_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const DOWNLOAD_TOTAL_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// Download the pinned standalone Python and create the shared user-space venv.
/// Returns the existing venv immediately when it has already been prepared.
pub(crate) fn ensure_user_python_venv<F: Fn(&str) + Send + Sync>(
    app: &AppHandle,
    on_progress: F,
) -> Result<PathBuf> {
    let python_bin = ensure_standalone_python(app, &on_progress)?;
    let env_dir = paths::qwen_env_dir(app)?;
    let venv_python = env_dir.join("bin/python");

    if let Some(parent) = env_dir.parent() {
        std::fs::create_dir_all(parent).context("Не удалось создать каталог окружения")?;
    }

    if !venv_python.exists() {
        on_progress("Создаю Python venv…");
        let out = Command::new(&python_bin)
            .arg("-m")
            .arg("venv")
            .arg(&env_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .context("Не удалось запустить python -m venv")?;
        if !out.status.success() {
            return Err(command_error(
                "Не удалось создать venv",
                &out.stderr,
                out.status.code(),
            ));
        }
    } else {
        on_progress("Использую существующий venv");
    }

    Ok(venv_python)
}

fn ensure_standalone_python<F: Fn(&str) + Send + Sync>(
    app: &AppHandle,
    on_progress: &F,
) -> Result<PathBuf> {
    let python_dir = paths::qwen_python_dir(app)?;
    let python_bin = python_dir.join("bin/python3.12");

    if python_bin.exists() {
        return Ok(python_bin);
    }

    let parent = python_dir
        .parent()
        .ok_or_else(|| anyhow!("Невалидный путь для Python"))?;
    std::fs::create_dir_all(parent).context("Не удалось создать каталог .qwen-mlx")?;

    let tarball_path = parent.join("python-download.tar.gz");
    on_progress(&format!(
        "Скачиваю Python 3.12 (~{} МБ)…",
        STANDALONE_PYTHON_BYTES / 1_048_576
    ));
    download_with_progress(STANDALONE_PYTHON_URL, &tarball_path, on_progress)
        .context("Не удалось скачать Python")?;

    on_progress("Проверяю SHA256…");
    let actual_sha = sha256_file(&tarball_path).context("Не удалось посчитать SHA256")?;
    if !actual_sha.eq_ignore_ascii_case(STANDALONE_PYTHON_SHA256) {
        let _ = std::fs::remove_file(&tarball_path);
        return Err(anyhow!(
            "SHA256 скачанного Python не совпадает (ожидалось {}, получено {}). \
             Возможно, файл повреждён — попробуйте ещё раз.",
            STANDALONE_PYTHON_SHA256,
            actual_sha
        ));
    }

    on_progress("Распаковываю Python…");
    extract_tar_gz(&tarball_path, parent).context("Не удалось распаковать Python")?;
    let _ = std::fs::remove_file(&tarball_path);

    if !python_bin.exists() {
        return Err(anyhow!(
            "Python распакован, но {} не найден. Архив повреждён или формат изменился.",
            python_bin.display()
        ));
    }

    let _ = Command::new("xattr")
        .args(["-dr", "com.apple.quarantine"])
        .arg(&python_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    Ok(python_bin)
}

fn download_with_progress<F: Fn(&str) + Send + Sync>(
    url: &str,
    dest: &Path,
    on_progress: &F,
) -> Result<()> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(DOWNLOAD_CONNECT_TIMEOUT)
        .timeout(DOWNLOAD_TOTAL_TIMEOUT)
        .build()?;
    let mut response = client
        .get(url)
        .send()
        .with_context(|| format!("HTTP GET failed: {url}"))?
        .error_for_status()?;
    let total = response.content_length().unwrap_or(STANDALONE_PYTHON_BYTES);
    let mut file = File::create(dest)
        .with_context(|| format!("Не удалось создать файл {}", dest.display()))?;
    let mut downloaded: u64 = 0;
    let mut buf = [0u8; 65_536];
    let mut last_pct: i32 = -1;
    loop {
        let n = response.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        downloaded += n as u64;
        let pct = ((downloaded as f64 / total as f64) * 100.0) as i32;
        if pct != last_pct && pct % 5 == 0 {
            on_progress(&format!(
                "Скачиваю Python: {} / {} МБ",
                downloaded / 1_048_576,
                total / 1_048_576
            ));
            last_pct = pct;
        }
    }
    file.flush()?;
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65_536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn extract_tar_gz(archive: &Path, dest_parent: &Path) -> Result<()> {
    let file = File::open(archive)?;
    let gz = GzDecoder::new(file);
    let mut tar = Archive::new(gz);
    tar.set_preserve_permissions(true);
    tar.unpack(dest_parent)?;
    Ok(())
}

fn command_error(prefix: &str, stderr: &[u8], code: Option<i32>) -> anyhow::Error {
    let tail = String::from_utf8_lossy(stderr)
        .lines()
        .rev()
        .take(20)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    if tail.trim().is_empty() {
        anyhow!("{prefix}. Код завершения: {code:?}")
    } else {
        anyhow!("{prefix}: {}", tail.trim())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_file_matches_known_digest() {
        let path = std::env::temp_dir().join(format!("parrot-mlx-env-sha-{}", std::process::id()));
        std::fs::write(&path, b"parrot").expect("write fixture");
        assert_eq!(
            sha256_file(&path).expect("hash fixture"),
            "4488b8b86b1ac061dbe37242297e5827dad889823fd1a5acaed43dec0108d048"
        );
        let _ = std::fs::remove_file(path);
    }
}
