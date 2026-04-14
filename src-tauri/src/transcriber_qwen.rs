use anyhow::{anyhow, Result};
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use tauri::AppHandle;

use crate::cancellation::CancelToken;
use crate::paths;

pub const ENGINE_QWEN_0_6B: &str = "qwen-0.6b";
pub const ENGINE_QWEN_1_7B: &str = "qwen-1.7b";

const MODEL_QWEN_0_6B: &str = "Qwen/Qwen3-ASR-0.6B";
const MODEL_QWEN_1_7B: &str = "Qwen/Qwen3-ASR-1.7B";

/// Expected cache size per model (safetensors + tokenizer + config).
/// Used to detect incomplete downloads.
pub const EXPECTED_QWEN_0_6B_BYTES: u64 = 1_100_000_000;
pub const EXPECTED_QWEN_1_7B_BYTES: u64 = 3_300_000_000;

pub fn is_qwen_engine(engine: &str) -> bool {
    model_for_engine(engine).is_some()
}

pub fn model_for_engine(engine: &str) -> Option<&'static str> {
    match engine {
        ENGINE_QWEN_0_6B => Some(MODEL_QWEN_0_6B),
        ENGINE_QWEN_1_7B => Some(MODEL_QWEN_1_7B),
        _ => None,
    }
}

pub fn is_ready(app: &AppHandle, engine: &str) -> bool {
    resolve_cli().is_ok() && model_cache_exists(app, engine)
}

pub fn warmup_model(app: &AppHandle, engine: &str) -> Result<()> {
    let model = model_for_engine(engine)
        .ok_or_else(|| anyhow!("unknown Qwen MLX engine: {engine}"))?;
    let cli = resolve_cli()?;
    let cache_dir = paths::qwen_cache_dir(app)?;
    let tmp = paths::tmp_dir(app)?;
    let warmup_wav = tmp.join(format!("qwen-warmup-{}.wav", engine.replace('.', "-")));

    write_silent_wav(&warmup_wav)?;
    let output = qwen_command(&cli, &cache_dir)
        .arg(&warmup_wav)
        .arg("--model")
        .arg(model)
        .arg("--stdout-only")
        .arg("--no-progress")
        .output();
    let _ = std::fs::remove_file(&warmup_wav);

    let output = output?;
    if !output.status.success() {
        return Err(command_error(
            "Qwen MLX не смог подготовить модель",
            &output.stderr,
            output.status.code(),
        ));
    }
    Ok(())
}

pub fn transcribe_wav(
    app: &AppHandle,
    wav_path: &Path,
    engine: &str,
    cancel: Option<Arc<CancelToken>>,
    progress_cb: impl Fn(u32) + Send + Sync,
) -> Result<String> {
    let model = model_for_engine(engine)
        .ok_or_else(|| anyhow!("unknown Qwen MLX engine: {engine}"))?;
    let cli = resolve_cli()?;
    let cache_dir = paths::qwen_cache_dir(app)?;

    progress_cb(5);
    let child = qwen_command(&cli, &cache_dir)
        .arg(wav_path)
        .arg("--model")
        .arg(model)
        .arg("--stdout-only")
        .arg("--no-progress")
        .spawn()?;

    let pid = child.id();
    if let Some(tok) = cancel.as_ref() {
        tok.register_pid(pid);
    }
    let output = child.wait_with_output()?;
    if let Some(tok) = cancel.as_ref() {
        tok.unregister_pid(pid);
    }

    if !output.status.success() {
        if cancel.as_ref().map(|t| t.is_cancelled()).unwrap_or(false) {
            return Err(anyhow!("cancelled"));
        }
        return Err(command_error(
            "Qwen MLX не смог расшифровать аудио",
            &output.stderr,
            output.status.code(),
        ));
    }

    progress_cb(100);
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn model_cache_exists(app: &AppHandle, engine: &str) -> bool {
    let Some(model) = model_for_engine(engine) else {
        return false;
    };
    let Ok(cache_dir) = paths::qwen_cache_dir(app) else {
        return false;
    };
    let repo_cache_name = format!("models--{}", model.replace('/', "--"));
    let model_dir = cache_dir.join("hub").join(repo_cache_name);
    if !model_dir.exists() {
        return false;
    }
    // Guard against partial downloads: require at least 90% of expected size.
    let expected = match engine {
        ENGINE_QWEN_0_6B => EXPECTED_QWEN_0_6B_BYTES,
        ENGINE_QWEN_1_7B => EXPECTED_QWEN_1_7B_BYTES,
        _ => return false,
    };
    dir_size(&model_dir) >= (expected as f64 * 0.9) as u64
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

fn resolve_cli() -> Result<PathBuf> {
    if let Some(path) =
        env::var_os("PARROT_QWEN_BIN").or_else(|| env::var_os("AUDIO_TO_TEXT_QWEN_BIN"))
    {
        let path = PathBuf::from(path);
        if path.exists() {
            return Ok(path);
        }
        return Err(anyhow!(
            "Qwen MLX не найден по указанному пути: {}",
            path.display()
        ));
    }

    for root in candidate_roots() {
        let path = root.join(".qwen-mlx/venv/bin/mlx-qwen3-asr");
        if path.exists() {
            return Ok(path);
        }
    }

    if let Some(path) = find_in_path("mlx-qwen3-asr") {
        return Ok(path);
    }

    Err(anyhow!(
        "Qwen MLX не установлен. Запустите tools/setup_qwen_mlx.sh или укажите PARROT_QWEN_BIN."
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

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    env::split_paths(&path_var)
        .map(|part| part.join(name))
        .find(|path| path.exists())
}

fn qwen_command(cli: &Path, cache_dir: &Path) -> Command {
    let mut command = Command::new(cli);
    command
        .env("HF_HOME", cache_dir)
        .env("HF_HUB_DISABLE_TELEMETRY", "1")
        .env("HF_HUB_DISABLE_XET", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
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

fn write_silent_wav(path: &Path) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for _ in 0..16_000 {
        writer.write_sample::<i16>(0)?;
    }
    writer.finalize()?;
    Ok(())
}
