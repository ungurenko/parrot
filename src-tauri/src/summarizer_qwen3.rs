use anyhow::{anyhow, Context, Result};
use flate2::read::GzDecoder;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::env;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use tar::Archive;
use tauri::AppHandle;

use crate::cancellation::CancelToken;
use crate::paths;
use crate::prompts;

// Pinned standalone Python release from astral-sh/python-build-standalone.
// To update: pick newer tag at https://github.com/astral-sh/python-build-standalone/releases,
// fetch matching SHA256 from SHA256SUMS in that release, replace both constants.
// Tarball expands to a top-level `python/` directory (~17.8 MB compressed, ~60 MB unpacked).
const STANDALONE_PYTHON_URL: &str = "https://github.com/astral-sh/python-build-standalone/releases/download/20260414/cpython-3.12.13+20260414-aarch64-apple-darwin-install_only.tar.gz";
const STANDALONE_PYTHON_SHA256: &str =
    "8966b2bcd9fa03ba22c080ad15a86bc12e41a00122b16f4b3740e302261124d9";
const STANDALONE_PYTHON_BYTES: u64 = 17_836_558;

pub const SUMMARY_MODEL_REPO: &str = "mlx-community/Qwen3-4B-Instruct-2507-4bit";
pub const EXPECTED_SUMMARY_BYTES: u64 = 2_300_000_000;
const READY_MARKER: &str = ".parrot-ready-summary";

const SUMMARY_MAX_TOKENS: &str = "4096";
const SUMMARY_TEMP: &str = "0.3";
const SUMMARY_TOP_P: &str = "0.9";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SummarizerStatus {
    pub available: bool,
    pub model_ready: bool,
    pub unavailable_reason: Option<String>,
}

pub fn status(app: &AppHandle) -> SummarizerStatus {
    let unavailable_reason = match resolve_python(app) {
        Ok(_) => None,
        Err(e) => Some(e.to_string()),
    };
    let model_ready = unavailable_reason.is_none() && model_cache_exists(app);
    SummarizerStatus {
        available: unavailable_reason.is_none(),
        model_ready,
        unavailable_reason,
    }
}

pub fn is_ready(app: &AppHandle) -> bool {
    resolve_python(app).is_ok() && model_cache_exists(app)
}

pub fn model_cache_exists(app: &AppHandle) -> bool {
    let Ok(cache_dir) = paths::qwen_cache_dir(app) else {
        return false;
    };
    if !ready_marker_exists(&cache_dir) {
        return false;
    }
    model_cache_exists_in(&cache_dir)
}

fn model_cache_exists_in(cache_dir: &Path) -> bool {
    let repo_cache_name = format!("models--{}", SUMMARY_MODEL_REPO.replace('/', "--"));
    let model_dir = cache_dir.join("hub").join(repo_cache_name);
    if !model_dir.exists() {
        return false;
    }
    dir_size(&model_dir) >= (EXPECTED_SUMMARY_BYTES as f64 * 0.9) as u64
}

pub fn delete_model(app: &AppHandle) -> Result<()> {
    let cache_dir = paths::qwen_cache_dir(app)?;
    remove_ready_marker(&cache_dir);
    let repo_cache_name = format!("models--{}", SUMMARY_MODEL_REPO.replace('/', "--"));
    let model_dir = cache_dir.join("hub").join(repo_cache_name);
    if model_dir.is_dir() {
        std::fs::remove_dir_all(&model_dir)?;
    }
    Ok(())
}

/// Download the model weights by running a short generation that triggers HF download.
pub fn warmup_model(app: &AppHandle, cancel: Arc<CancelToken>) -> Result<()> {
    let python = resolve_python(app)?;
    let cache_dir = paths::qwen_cache_dir(app)?;

    if cancel.is_cancelled() {
        anyhow::bail!("cancelled");
    }
    let child = mlx_lm_command(&python, &cache_dir)
        .arg("generate")
        .arg("--model")
        .arg(SUMMARY_MODEL_REPO)
        .arg("--prompt")
        .arg("Привет")
        .arg("--max-tokens")
        .arg("4")
        .arg("--verbose")
        .arg("False")
        .stdin(Stdio::null())
        .spawn()?;

    let pid = child.id();
    cancel.register_pid(pid);
    let output = child.wait_with_output()?;
    cancel.unregister_pid(pid);
    if !output.status.success() {
        if cancel.is_cancelled() {
            anyhow::bail!("cancelled");
        }
        return Err(command_error(
            "Не удалось подготовить модель конспекта",
            &output.stderr,
            output.status.code(),
        ));
    }
    write_ready_marker(&cache_dir)?;
    Ok(())
}

pub fn generate_summary(
    app: &AppHandle,
    transcript: &str,
    cancel: Arc<CancelToken>,
) -> Result<String> {
    let python = resolve_python(app)?;
    let cache_dir = paths::qwen_cache_dir(app)?;

    let system_prompt = prompts::SUMMARY_SYSTEM_PROMPT;
    let user_prompt = prompts::build_summary_user_prompt(transcript);

    let mut child = mlx_lm_command(&python, &cache_dir)
        .arg("generate")
        .arg("--model")
        .arg(SUMMARY_MODEL_REPO)
        .arg("--system-prompt")
        .arg(system_prompt)
        .arg("--prompt")
        .arg("-")
        .arg("--max-tokens")
        .arg(SUMMARY_MAX_TOKENS)
        .arg("--temp")
        .arg(SUMMARY_TEMP)
        .arg("--top-p")
        .arg(SUMMARY_TOP_P)
        .arg("--verbose")
        .arg("False")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Не удалось запустить mlx-lm")?;

    let pid = child.id();
    cancel.register_pid(pid);

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("mlx-lm не открыл stdin"))?;
    stdin
        .write_all(user_prompt.as_bytes())
        .context("Не удалось передать промпт в stdin mlx-lm")?;
    drop(stdin);

    let output = child.wait_with_output()?;
    cancel.unregister_pid(pid);

    if !output.status.success() {
        if cancel.is_cancelled() {
            return Err(anyhow!("cancelled"));
        }
        return Err(command_error(
            "Модель конспекта завершилась с ошибкой",
            &output.stderr,
            output.status.code(),
        ));
    }

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return Err(anyhow!("Модель конспекта вернула пустой ответ"));
    }
    Ok(text)
}

fn mlx_lm_command(python: &Path, cache_dir: &Path) -> Command {
    let mut cmd = Command::new(python);
    cmd.arg("-m").arg("mlx_lm");
    cmd.env("HF_HOME", cache_dir)
        .env("HF_HUB_DISABLE_TELEMETRY", "1")
        .env("HF_HUB_DISABLE_XET", "1")
        .env("PYTHONUNBUFFERED", "1");
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

fn resolve_python(app: &AppHandle) -> Result<PathBuf> {
    if let Some(path) = env::var_os("PARROT_QWEN_PYTHON") {
        let p = PathBuf::from(path);
        if p.exists() {
            return Ok(p);
        }
    }
    if let Some(path) =
        env::var_os("PARROT_QWEN_BIN").or_else(|| env::var_os("AUDIO_TO_TEXT_QWEN_BIN"))
    {
        let p = PathBuf::from(path);
        if let Some(parent) = p.parent() {
            let py = parent.join("python");
            if py.exists() {
                return Ok(py);
            }
        }
    }

    // User-space venv created by `setup_summarizer_env` Tauri command —
    // works for users who installed Parrot from the .dmg and have no repo.
    if let Ok(env_dir) = paths::qwen_env_dir(app) {
        let py = env_dir.join("bin/python");
        if py.exists() {
            return Ok(py);
        }
    }

    // Repo-local venv created by `tools/setup_qwen_mlx.sh` — the dev path.
    for root in candidate_roots() {
        let path = root.join(".qwen-mlx/venv/bin/python");
        if path.exists() {
            return Ok(path);
        }
    }

    Err(anyhow!(
        "Python venv для конспекта не установлен. Нажмите «Установить окружение» в настройках."
    ))
}

fn candidate_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(current) = env::current_dir() {
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

/// Download + verify + extract astral-sh/python-build-standalone if not yet
/// installed. Returns path to the extracted `bin/python3.12`. Idempotent.
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

    // Download to a temp file inside the same parent (atomic-ish: same FS,
    // we move/rename only after verification + extraction succeed).
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

    // Strip macOS quarantine xattr so the freshly extracted binaries can run
    // without Gatekeeper prompts. Best-effort; ignore failures.
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
    let mut response = reqwest::blocking::get(url)
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

/// Bootstrap the user-space MLX venv: ensure Python is downloaded, create
/// the venv, upgrade pip, install mlx-lm. Idempotent — if everything is
/// already present, returns quickly. Streams progress lines via `on_progress`.
pub fn install_env<F: Fn(&str) + Send + Sync>(app: &AppHandle, on_progress: F) -> Result<()> {
    // Step 0 — ensure standalone Python is on disk (downloads on first run).
    let python_bin = ensure_standalone_python(app, &on_progress)?;

    let env_dir = paths::qwen_env_dir(app)?;
    let venv_python = env_dir.join("bin/python");

    if let Some(parent) = env_dir.parent() {
        std::fs::create_dir_all(parent).context("Не удалось создать каталог окружения")?;
    }

    // Step 1 — create venv if missing.
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

    // Step 2 — upgrade pip (best-effort; skip failure to keep going).
    on_progress("Обновляю pip…");
    let _ = Command::new(&venv_python)
        .args(["-m", "pip", "install", "--upgrade", "pip"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    // Step 3 — install mlx-lm (the only Python dep the summarizer needs).
    on_progress("Устанавливаю mlx-lm (~50 МБ)…");
    let out = Command::new(&venv_python)
        .args([
            "-m",
            "pip",
            "install",
            "--disable-pip-version-check",
            "mlx-lm>=0.24.0",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("Не удалось запустить pip install mlx-lm")?;
    if !out.status.success() {
        return Err(command_error(
            "pip install mlx-lm завершился с ошибкой",
            &out.stderr,
            out.status.code(),
        ));
    }

    // Step 4 — sanity check: import mlx_lm.
    on_progress("Проверяю установку…");
    let out = Command::new(&venv_python)
        .args(["-c", "import mlx_lm; print(mlx_lm.__version__)"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("Не удалось проверить mlx_lm")?;
    if !out.status.success() {
        return Err(command_error(
            "mlx_lm не импортируется после установки",
            &out.stderr,
            out.status.code(),
        ));
    }

    on_progress("Готово");
    Ok(())
}

fn dir_size(path: &Path) -> u64 {
    let mut total: u64 = 0;
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() {
                total = total.saturating_add(meta.len());
            } else if meta.is_dir() {
                total = total.saturating_add(dir_size(&p));
            }
        }
    }
    total
}

fn write_ready_marker(cache_dir: &Path) -> Result<()> {
    std::fs::write(cache_dir.join(READY_MARKER), b"ok")?;
    Ok(())
}

fn remove_ready_marker(cache_dir: &Path) {
    let _ = std::fs::remove_file(cache_dir.join(READY_MARKER));
}

fn ready_marker_exists(cache_dir: &Path) -> bool {
    cache_dir.join(READY_MARKER).is_file()
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

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "parrot-summary-ready-test-{}-{}",
            name,
            std::process::id()
        ))
    }

    #[test]
    fn ready_marker_controls_summary_readiness() {
        let dir = temp_dir("marker");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");

        assert!(!ready_marker_exists(&dir));
        write_ready_marker(&dir).expect("write marker");
        assert!(ready_marker_exists(&dir));
        remove_ready_marker(&dir);
        assert!(!ready_marker_exists(&dir));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
