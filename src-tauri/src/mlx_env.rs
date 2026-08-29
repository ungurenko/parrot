use anyhow::{anyhow, Context, Result};
use flate2::read::GzDecoder;
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::LazyLock;
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
static INSTALL_LOCK: Mutex<()> = Mutex::new(());

// Parrot uses one shared venv for Qwen ASR and local summaries. Keep the MLX
// pin here so installing either feature cannot silently break the other one.
pub(crate) const SHARED_MLX_PACKAGE: &str = "mlx==0.31.2";

pub(crate) fn shared_mlx_version() -> &'static str {
    SHARED_MLX_PACKAGE
        .strip_prefix("mlx==")
        .expect("shared MLX package must be pinned with ==")
}

// Shared warm-server HTTP plumbing (Qwen ASR and summary servers).
pub(crate) const SERVER_HOST: &str = "127.0.0.1";
pub(crate) const SERVER_START_TIMEOUT: Duration = Duration::from_secs(90);
pub(crate) const SERVER_REQUEST_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const HEALTH_TIMEOUT: Duration = Duration::from_millis(700);
static HEALTH_CLIENT: LazyLock<Result<reqwest::blocking::Client, reqwest::Error>> =
    LazyLock::new(|| {
        reqwest::blocking::Client::builder()
            .timeout(HEALTH_TIMEOUT)
            .build()
    });

pub(crate) fn health_ok(port: u16) -> bool {
    let Ok(client) = HEALTH_CLIENT.as_ref() else {
        return false;
    };
    client
        .get(format!("http://{SERVER_HOST}:{port}/health"))
        .send()
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

const STOP_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) fn signal_process(pid: u32, signal: libc::c_int) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let result = unsafe { libc::kill(pid as libc::pid_t, signal) };
        if result == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (pid, signal);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "process signals are unavailable on this platform",
        ))
    }
}

/// Terminate a warm-server child: SIGTERM, wait up to [`STOP_TIMEOUT`], then
/// SIGKILL. Returns a human-readable outcome for logs.
pub(crate) fn stop_child(child: &mut std::process::Child) -> String {
    use std::time::Instant;

    let pid = child.id();
    match child.try_wait() {
        Ok(Some(status)) => return format!("already exited ({status})"),
        Ok(None) => {}
        Err(error) => return format!("wait before stop failed: {error}"),
    }

    let term_result = signal_process(pid, libc::SIGTERM);
    let deadline = Instant::now() + STOP_TIMEOUT;
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(status)) => return format!("SIGTERM -> {status}"),
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(error) => return format!("wait after SIGTERM failed: {error}"),
        }
    }

    let kill_result = child.kill();
    let wait_result = child.wait();
    format!("SIGTERM={term_result:?}, SIGKILL={kill_result:?}, wait={wait_result:?}")
}

/// Dev-mode lookup roots for repo-local MLX venvs (cwd, its parent, crate dir
/// and its parent). Production installs resolve through Application Support.
pub(crate) fn candidate_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(current) = std::env::current_dir() {
        roots.push(current.clone());
        if let Some(parent) = current.parent() {
            roots.push(parent.to_path_buf());
        }
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    roots.push(manifest_dir.clone());
    if let Some(parent) = manifest_dir.parent() {
        roots.push(parent.to_path_buf());
    }
    roots
}

/// Run `<python> -m pip install <args>` with the shared stdio plumbing.
pub(crate) fn pip_install(
    python: &Path,
    args: &[&str],
    context: &str,
) -> Result<std::process::Output> {
    Command::new(python)
        .args(["-m", "pip", "install"])
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context(context.to_string())
}

/// Base command for MLX Python CLIs: HF cache pointed at the shared cache
/// dir, hub telemetry/xet off, unbuffered output.
pub(crate) fn python_command(python: &Path, cache_dir: &Path) -> Command {
    let mut cmd = Command::new(python);
    cmd.env("HF_HOME", cache_dir)
        .env("HF_HUB_DISABLE_TELEMETRY", "1")
        .env("HF_HUB_DISABLE_XET", "1")
        .env("PYTHONUNBUFFERED", "1");
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

/// HuggingFace hub cache directory name for a repo id (`org/name`).
pub(crate) fn repo_cache_name(repo: &str) -> String {
    format!("models--{}", repo.replace('/', "--"))
}

/// True when the repo's hub cache holds ≥90% of the expected bytes — guards
/// against running on partially downloaded weights.
pub(crate) fn model_cache_ready(app: &AppHandle, repo: &str, expected_bytes: u64) -> bool {
    let Ok(cache_dir) = paths::qwen_cache_dir(app) else {
        return false;
    };
    let model_dir = cache_dir.join("hub").join(repo_cache_name(repo));
    model_dir.exists()
        && crate::fs_metrics::dir_size_bytes(&model_dir) >= (expected_bytes as f64 * 0.9) as u64
}

/// Cheap `-c "<code>"` probe; true when the interpreter exits successfully.
pub(crate) fn python_import_ok(python: &Path, code: &str) -> bool {
    Command::new(python)
        .args(["-c", code])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Serialize all mutations of the shared MLX venv. Parakeet, Qwen ASR and
/// summaries use the same Python environment, and parallel pip processes can
/// otherwise leave its metadata or packages in a partially updated state.
pub(crate) fn with_install_lock<T>(operation: impl FnOnce() -> Result<T>) -> Result<T> {
    let _guard = INSTALL_LOCK.lock();
    operation()
}

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

pub(crate) fn command_error(prefix: &str, stderr: &[u8], code: Option<i32>) -> anyhow::Error {
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

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

    #[test]
    fn install_lock_serializes_shared_venv_mutations() {
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();

        for _ in 0..4 {
            let active = active.clone();
            let max_active = max_active.clone();
            handles.push(std::thread::spawn(move || {
                with_install_lock(|| {
                    let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                    max_active.fetch_max(now, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(10));
                    active.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                })
                .expect("install lock")
            }));
        }

        for handle in handles {
            handle.join().expect("join install worker");
        }
        assert_eq!(max_active.load(Ordering::SeqCst), 1);
    }
}
