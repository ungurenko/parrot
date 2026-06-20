use std::path::PathBuf;

use serde::Serialize;
use tauri::State;

use crate::queue::{self, Job, SourceKind};
use crate::{settings, source, AppState};

#[derive(Serialize)]
pub(crate) struct EnqueueResult {
    id: String,
}

#[tauri::command]
pub(crate) async fn enqueue_file(
    path: String,
    engine: Option<String>,
    language: Option<String>,
    state: State<'_, AppState>,
) -> Result<EnqueueResult, String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("Файл не найден: {path}"));
    }
    if !source::is_supported_file(&p) {
        return Err("Неподдерживаемый формат файла".into());
    }
    let settings = settings::load(&state.queue.app_handle());
    let engine = engine.unwrap_or_else(|| settings.engine.clone());
    let language = language.unwrap_or_else(|| settings.language.clone());
    validate_job_options(&engine, &language)?;
    let id = queue::new_job_id();
    let display_name = p
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.clone());
    state
        .queue
        .enqueue(Job {
            id: id.clone(),
            source: SourceKind::LocalFile(p),
            display_name,
            engine,
            language,
            save_dir: settings.save_dir,
        })
        .await
        .map_err(|e| e.to_string())?;
    Ok(EnqueueResult { id })
}

#[tauri::command]
pub(crate) async fn enqueue_youtube(
    url: String,
    engine: Option<String>,
    language: Option<String>,
    state: State<'_, AppState>,
) -> Result<EnqueueResult, String> {
    let trimmed = url.trim().to_string();
    if !source::is_youtube_url(&trimmed) {
        return Err("URL не похож на YouTube-ссылку".into());
    }
    let settings = settings::load(&state.queue.app_handle());
    let engine = engine.unwrap_or_else(|| settings.engine.clone());
    let language = language.unwrap_or_else(|| settings.language.clone());
    validate_job_options(&engine, &language)?;
    let id = queue::new_job_id();
    state
        .queue
        .enqueue(Job {
            id: id.clone(),
            source: SourceKind::YouTube(trimmed.clone()),
            display_name: trimmed,
            engine,
            language,
            save_dir: settings.save_dir,
        })
        .await
        .map_err(|e| e.to_string())?;
    Ok(EnqueueResult { id })
}

#[tauri::command]
pub(crate) fn cancel_job(id: String, state: State<'_, AppState>) -> bool {
    state.queue.cancel_registry().cancel(&id)
}

fn validate_job_options(engine: &str, language: &str) -> Result<(), String> {
    if !settings::is_supported_engine(engine) {
        return Err(format!("Неизвестная модель распознавания: {engine}"));
    }
    if !settings::is_supported_language(language) {
        return Err(format!("Неизвестный язык аудио: {language}"));
    }
    Ok(())
}
