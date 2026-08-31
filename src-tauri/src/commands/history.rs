use tauri::{AppHandle, Emitter};

use crate::{history, validate_saved_file_path, SavedFileKind};

#[tauri::command]
pub(crate) fn get_history(app: AppHandle) -> Vec<history::HistoryEntry> {
    history::load(&app)
}

#[tauri::command]
pub(crate) fn clear_history(app: AppHandle) -> Result<(), String> {
    history::clear(&app).map_err(|e| e.to_string())?;
    let _ = app.emit("history:updated", ());
    Ok(())
}

#[tauri::command]
pub(crate) fn delete_history_entry(id: String, app: AppHandle) -> Result<(), String> {
    history::remove(&app, &id).map_err(|e| e.to_string())?;
    let _ = app.emit("history:updated", ());
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LoadedHistoryEntry {
    entry: history::HistoryEntry,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    translation: Option<String>,
}

#[tauri::command]
pub(crate) fn load_history_entry(id: String, app: AppHandle) -> Result<LoadedHistoryEntry, String> {
    let entry = history::get(&app, &id).ok_or_else(|| "Запись не найдена".to_string())?;
    let transcript_path =
        validate_saved_file_path(&app, &entry.output_path, SavedFileKind::Transcript)?;
    let text = std::fs::read_to_string(&transcript_path)
        .map_err(|e| format!("Не удалось прочитать транскрипт: {e}"))?;
    let summary = entry
        .summary_path
        .as_ref()
        .and_then(|p| validate_saved_file_path(&app, p, SavedFileKind::Summary).ok())
        .and_then(|p| std::fs::read_to_string(p).ok());
    let translation = entry
        .translation_path
        .as_ref()
        .and_then(|p| validate_saved_file_path(&app, p, SavedFileKind::Translation).ok())
        .and_then(|p| std::fs::read_to_string(p).ok());
    Ok(LoadedHistoryEntry {
        entry,
        text,
        summary,
        translation,
    })
}
