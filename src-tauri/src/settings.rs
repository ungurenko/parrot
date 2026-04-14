use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::AppHandle;

use crate::paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub save_dir: PathBuf,
    pub onboarded: bool,
    #[serde(default = "default_engine")]
    pub engine: String,
}

fn default_engine() -> String {
    "qwen-0.6b".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            save_dir: paths::default_save_dir(),
            onboarded: false,
            engine: default_engine(),
        }
    }
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
        Some(s) => s,
        None => Settings::default(),
    }
}

pub fn save(app: &AppHandle, settings: &Settings) -> Result<()> {
    let path = paths::settings_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(settings)?)?;
    Ok(())
}
