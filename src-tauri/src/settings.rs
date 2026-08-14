use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::AppHandle;

use crate::{paths, summarizer_models};

pub const DEFAULT_ENGINE: &str = "parakeet";
pub const DEFAULT_LANGUAGE: &str = "ru";
pub const DEFAULT_DICTATION_SHORTCUT: &str = "Alt+Space";
pub const SUPPORTED_ENGINES: [&str; 4] = ["parakeet", "qwen-0.6b", "qwen-1.7b", "whisper"];
pub const SUPPORTED_LANGUAGES: [&str; 9] = ["auto", "ru", "en", "de", "fr", "es", "it", "pt", "uk"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub save_dir: PathBuf,
    pub onboarded: bool,
    #[serde(default = "default_engine")]
    pub engine: String,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub summarizer_enabled: bool,
    #[serde(default = "default_summary_model")]
    pub summary_model: String,
    #[serde(default)]
    pub summarizer_promo_seen: bool,
    #[serde(default = "default_dictation_enabled")]
    pub dictation_enabled: bool,
    #[serde(default = "default_dictation_hold_key")]
    pub dictation_hold_key: String,
}

fn default_engine() -> String {
    DEFAULT_ENGINE.to_string()
}

fn default_language() -> String {
    DEFAULT_LANGUAGE.to_string()
}

fn default_summary_model() -> String {
    summarizer_models::DEFAULT_SUMMARY_MODEL.to_string()
}

fn default_dictation_enabled() -> bool {
    true
}

fn default_dictation_hold_key() -> String {
    DEFAULT_DICTATION_SHORTCUT.to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            save_dir: paths::default_save_dir(),
            onboarded: false,
            engine: default_engine(),
            language: default_language(),
            summarizer_enabled: false,
            summary_model: default_summary_model(),
            summarizer_promo_seen: false,
            dictation_enabled: default_dictation_enabled(),
            dictation_hold_key: default_dictation_hold_key(),
        }
    }
}

impl Settings {
    pub fn normalized(mut self) -> Self {
        if self.save_dir.as_os_str().is_empty() {
            self.save_dir = paths::default_save_dir();
        }
        if !is_supported_engine(&self.engine) {
            self.engine = default_engine();
        }
        if !is_supported_language(&self.language) {
            self.language = default_language();
        }
        self.summary_model =
            summarizer_models::normalize_summary_model(&self.summary_model).to_string();
        if !is_valid_dictation_shortcut(&self.dictation_hold_key) {
            self.dictation_hold_key = default_dictation_hold_key();
        }
        self
    }
}

pub fn is_supported_engine(engine: &str) -> bool {
    SUPPORTED_ENGINES.contains(&engine)
}

pub fn is_supported_language(language: &str) -> bool {
    SUPPORTED_LANGUAGES.contains(&language)
}

pub fn validate_for_save(settings: &Settings) -> Result<()> {
    if settings.save_dir.as_os_str().is_empty() {
        anyhow::bail!("Папка сохранения не выбрана.");
    }
    if settings.save_dir.exists() && !settings.save_dir.is_dir() {
        anyhow::bail!("Путь сохранения должен быть папкой.");
    }
    if !is_supported_engine(&settings.engine) {
        anyhow::bail!("Неизвестная модель распознавания: {}", settings.engine);
    }
    if !is_supported_language(&settings.language) {
        anyhow::bail!("Неизвестный язык аудио: {}", settings.language);
    }
    if !summarizer_models::is_supported_summary_model(&settings.summary_model) {
        anyhow::bail!("Неизвестная модель конспекта: {}", settings.summary_model);
    }
    if !is_valid_dictation_shortcut(&settings.dictation_hold_key) {
        anyhow::bail!("Некорректное сочетание клавиш для диктовки.");
    }
    Ok(())
}

fn is_valid_dictation_shortcut(shortcut: &str) -> bool {
    let parts: Vec<&str> = shortcut
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() < 2 {
        return false;
    }
    let Some(key) = parts.last() else {
        return false;
    };
    if is_modifier_key(key) {
        return false;
    }
    parts[..parts.len() - 1]
        .iter()
        .any(|part| is_modifier_key(part))
}

fn is_modifier_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "alt" | "option" | "control" | "ctrl" | "command" | "cmd" | "shift" | "super"
    )
}

pub fn load(app: &AppHandle) -> Settings {
    let path = match paths::settings_path(app) {
        Ok(p) => p,
        Err(_) => return Settings::default(),
    };
    if !path.exists() {
        return Settings::default();
    }
    match std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
    {
        Some(s) => s.normalized(),
        None => Settings::default(),
    }
}

pub fn save(app: &AppHandle, settings: &Settings) -> Result<()> {
    validate_for_save(settings)?;
    let path = paths::settings_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::create_dir_all(&settings.save_dir)?;
    std::fs::write(&path, serde_json::to_string_pretty(settings)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_should_restore_unknown_engine_and_language() {
        let settings = Settings {
            save_dir: PathBuf::from(""),
            onboarded: true,
            engine: "bad-engine".to_string(),
            language: "bad-language".to_string(),
            summarizer_enabled: false,
            summary_model: "bad-summary-model".to_string(),
            summarizer_promo_seen: false,
            dictation_enabled: true,
            dictation_hold_key: "bad-key".to_string(),
        }
        .normalized();

        assert_eq!(settings.engine, DEFAULT_ENGINE);
        assert_eq!(settings.language, DEFAULT_LANGUAGE);
        assert_eq!(
            settings.summary_model,
            summarizer_models::DEFAULT_SUMMARY_MODEL
        );
        assert_eq!(settings.dictation_hold_key, DEFAULT_DICTATION_SHORTCUT);
        assert!(!settings.save_dir.as_os_str().is_empty());
    }

    #[test]
    fn validate_for_save_should_reject_unknown_engine() {
        let settings = Settings {
            engine: "bad-engine".to_string(),
            ..Settings::default()
        };

        assert!(validate_for_save(&settings).is_err());
    }

    #[test]
    fn validate_for_save_should_reject_unknown_summary_model() {
        let settings = Settings {
            summary_model: "bad-summary-model".to_string(),
            ..Settings::default()
        };

        assert!(validate_for_save(&settings).is_err());
    }
}
