use anyhow::Result;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

const LEGACY_IDENTIFIER: &str = "com.alexk.audiototext";

pub fn app_data_dir(app: &AppHandle) -> Result<PathBuf> {
    Ok(app.path().app_data_dir()?)
}

pub fn migrate_legacy_app_data(app: &AppHandle) -> Result<()> {
    let current_dir = app_data_dir(app)?;
    let Some(base_dir) = current_dir.parent() else {
        return Ok(());
    };
    let legacy_dir = base_dir.join(LEGACY_IDENTIFIER);

    migrate_legacy_app_data_dirs(&legacy_dir, &current_dir)
}

fn migrate_legacy_app_data_dirs(legacy_dir: &Path, current_dir: &Path) -> Result<()> {
    if !legacy_dir.exists() || !is_dir_empty_or_missing(current_dir) {
        return Ok(());
    }

    let Some(base_dir) = current_dir.parent() else {
        return Ok(());
    };
    std::fs::create_dir_all(base_dir)?;
    if current_dir.exists() {
        std::fs::remove_dir(current_dir)?;
    }
    std::fs::rename(legacy_dir, current_dir)?;
    Ok(())
}

fn is_dir_empty_or_missing(path: &Path) -> bool {
    if !path.exists() {
        return true;
    }
    std::fs::read_dir(path)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(false)
}

pub fn model_path(app: &AppHandle) -> Result<PathBuf> {
    let dir = app_data_dir(app)?.join("models");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("ggml-large-v3-turbo-q5_0.bin"))
}

pub fn coreml_encoder_path(app: &AppHandle) -> Result<PathBuf> {
    let dir = app_data_dir(app)?.join("models");
    // whisper.cpp strips the "-qN_N" quant suffix, then appends "-encoder.mlmodelc".
    Ok(dir.join("ggml-large-v3-turbo-encoder.mlmodelc"))
}

pub fn parakeet_dir(app: &AppHandle) -> Result<PathBuf> {
    let dir = app_data_dir(app)?.join("models").join("parakeet-v3");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn qwen_cache_dir(app: &AppHandle) -> Result<PathBuf> {
    let dir = app_data_dir(app)?.join("models").join("qwen-mlx");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn qwen_env_dir(app: &AppHandle) -> Result<PathBuf> {
    Ok(app_data_dir(app)?.join(".qwen-mlx").join("venv"))
}

pub fn qwen_python_dir(app: &AppHandle) -> Result<PathBuf> {
    Ok(app_data_dir(app)?.join(".qwen-mlx").join("python"))
}

pub fn parakeet_files_ready(app: &AppHandle) -> bool {
    let Ok(dir) = parakeet_dir(app) else {
        return false;
    };
    if !file_has_content(&dir.join("vocab.txt")) {
        return false;
    }
    // Accept either the int8 variant (faster, default) or the fp32 variant.
    let has_encoder = file_has_content(&dir.join("encoder-model.int8.onnx"))
        || (file_has_content(&dir.join("encoder-model.onnx"))
            && file_has_content(&dir.join("encoder-model.onnx.data")));
    let has_decoder = file_has_content(&dir.join("decoder_joint-model.int8.onnx"))
        || file_has_content(&dir.join("decoder_joint-model.onnx"));
    has_encoder && has_decoder
}

fn file_has_content(path: &Path) -> bool {
    path.metadata()
        .map(|m| m.is_file() && m.len() > 0)
        .unwrap_or(false)
}

pub fn settings_path(app: &AppHandle) -> Result<PathBuf> {
    Ok(app_data_dir(app)?.join("settings.json"))
}

pub fn history_path(app: &AppHandle) -> Result<PathBuf> {
    Ok(app_data_dir(app)?.join("history.json"))
}

pub fn tmp_dir(app: &AppHandle) -> Result<PathBuf> {
    let dir = app.path().app_cache_dir()?.join("tmp");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn logs_dir(app: &AppHandle) -> Result<PathBuf> {
    let dir = app.path().app_log_dir()?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn default_save_dir() -> PathBuf {
    dirs::document_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
        .join("Transcripts")
}

pub fn cleanup_tmp(app: &AppHandle) {
    if let Ok(dir) = tmp_dir(app) {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::migrate_legacy_app_data_dirs;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "parrot-migration-test-{}-{}",
            name,
            std::process::id()
        ))
    }

    #[test]
    fn migration_moves_legacy_data_when_current_dir_is_missing() {
        let base = temp_dir("moves");
        let legacy = base.join("com.alexk.audiototext");
        let current = base.join("com.alexk.parrot");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&legacy).expect("create legacy dir");
        std::fs::write(legacy.join("settings.json"), "{}").expect("write settings");

        migrate_legacy_app_data_dirs(&legacy, &current).expect("migrate legacy data");

        assert!(current.join("settings.json").exists());
        assert!(!legacy.exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn migration_does_not_overwrite_current_data() {
        let base = temp_dir("keeps-current");
        let legacy = base.join("com.alexk.audiototext");
        let current = base.join("com.alexk.parrot");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&legacy).expect("create legacy dir");
        std::fs::create_dir_all(&current).expect("create current dir");
        std::fs::write(legacy.join("settings.json"), "{}").expect("write legacy settings");
        std::fs::write(current.join("settings.json"), "{\"fresh\":true}")
            .expect("write current settings");

        migrate_legacy_app_data_dirs(&legacy, &current).expect("skip migration");

        assert!(legacy.join("settings.json").exists());
        assert_eq!(
            std::fs::read_to_string(current.join("settings.json")).expect("read current"),
            "{\"fresh\":true}"
        );
        let _ = std::fs::remove_dir_all(&base);
    }
}
