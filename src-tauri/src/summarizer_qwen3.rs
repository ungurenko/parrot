use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use std::env;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use tauri::AppHandle;

use crate::cancellation::CancelToken;
use crate::paths;
use crate::prompts;

pub const SUMMARY_MODEL_REPO: &str = "mlx-community/Qwen3-4B-Instruct-2507-4bit";
pub const EXPECTED_SUMMARY_BYTES: u64 = 2_300_000_000;

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
    let unavailable_reason = match resolve_python() {
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
    resolve_python().is_ok() && model_cache_exists(app)
}

pub fn model_cache_exists(app: &AppHandle) -> bool {
    let Ok(cache_dir) = paths::qwen_cache_dir(app) else {
        return false;
    };
    let repo_cache_name = format!("models--{}", SUMMARY_MODEL_REPO.replace('/', "--"));
    let model_dir = cache_dir.join("hub").join(repo_cache_name);
    if !model_dir.exists() {
        return false;
    }
    dir_size(&model_dir) >= (EXPECTED_SUMMARY_BYTES as f64 * 0.9) as u64
}

pub fn delete_model(app: &AppHandle) -> Result<()> {
    let cache_dir = paths::qwen_cache_dir(app)?;
    let repo_cache_name = format!("models--{}", SUMMARY_MODEL_REPO.replace('/', "--"));
    let model_dir = cache_dir.join("hub").join(repo_cache_name);
    if model_dir.is_dir() {
        std::fs::remove_dir_all(&model_dir)?;
    }
    Ok(())
}

/// Download the model weights by running a short generation that triggers HF download.
pub fn warmup_model(app: &AppHandle) -> Result<()> {
    let python = resolve_python()?;
    let cache_dir = paths::qwen_cache_dir(app)?;

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

    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(command_error(
            "Не удалось подготовить модель конспекта",
            &output.stderr,
            output.status.code(),
        ));
    }
    Ok(())
}

pub fn generate_summary(
    app: &AppHandle,
    transcript: &str,
    cancel: Arc<CancelToken>,
) -> Result<String> {
    let python = resolve_python()?;
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

fn resolve_python() -> Result<PathBuf> {
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

    for root in candidate_roots() {
        let path = root.join(".qwen-mlx/venv/bin/python");
        if path.exists() {
            return Ok(path);
        }
    }

    Err(anyhow!(
        "Python venv для MLX не найден. Запустите tools/setup_qwen_mlx.sh."
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
