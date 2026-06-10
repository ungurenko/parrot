use std::path::Path;

use tauri::AppHandle;

use crate::{paths, validate_saved_file_path, SavedFileKind};

#[tauri::command]
pub(crate) fn open_in_finder(path: String, app: AppHandle) -> Result<(), String> {
    let kind = if crate::saved_file_kind_matches(Path::new(&path), SavedFileKind::Transcript) {
        SavedFileKind::Transcript
    } else {
        SavedFileKind::Summary
    };
    let path = validate_saved_file_path(&app, &path, kind)?;
    std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn open_logs(app: AppHandle) -> Result<(), String> {
    let dir = paths::logs_dir(&app).map_err(|e| e.to_string())?;
    std::process::Command::new("open")
        .arg(dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn log_client_error(scope: String, message: String) -> Result<(), String> {
    tracing::error!(scope = %scope, "client error: {}", message);
    Ok(())
}
