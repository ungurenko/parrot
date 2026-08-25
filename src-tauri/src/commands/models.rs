use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::cancellation;
use crate::commands::spawn_model_progress_poller;
use crate::{model, paths, preload_active_engine, settings, transcriber, transcriber_parakeet};
use crate::{transcriber_qwen, AppState, CancelRegistryGuard};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EngineStatus {
    available: bool,
    model_ready: bool,
    unavailable_reason: Option<String>,
}

#[tauri::command]
pub(crate) fn is_model_ready(app: AppHandle) -> bool {
    let s = settings::load(&app);
    is_model_ready_for_engine(&app, &s.engine)
}

#[tauri::command]
pub(crate) fn get_engine_statuses(app: AppHandle) -> HashMap<String, EngineStatus> {
    settings::SUPPORTED_ENGINES
        .into_iter()
        .map(|engine| (engine.to_string(), engine_status(&app, engine)))
        .collect()
}

fn engine_status(app: &AppHandle, engine: &str) -> EngineStatus {
    let unavailable_reason = engine_unavailable_reason(app, engine);
    EngineStatus {
        available: unavailable_reason.is_none(),
        model_ready: unavailable_reason.is_none() && is_model_ready_for_engine(app, engine),
        unavailable_reason,
    }
}

fn engine_unavailable_reason(_app: &AppHandle, engine: &str) -> Option<String> {
    if transcriber_qwen::is_qwen_engine(engine) {
        return transcriber_qwen::unavailable_reason();
    }
    None
}

pub(crate) fn is_model_ready_for_engine(app: &AppHandle, engine: &str) -> bool {
    match engine {
        "parakeet" => paths::parakeet_files_ready(app),
        "whisper" => paths::whisper_files_ready(app),
        engine if transcriber_qwen::is_qwen_engine(engine) => {
            transcriber_qwen::is_ready(app, engine)
        }
        _ => false,
    }
}

#[tauri::command]
pub(crate) async fn download_model(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let engine = settings::load(&app).engine;
    download_model_for_engine(app, state, engine).await
}

#[tauri::command]
pub(crate) async fn download_model_for_engine(
    app: AppHandle,
    state: State<'_, AppState>,
    engine: String,
) -> Result<(), String> {
    let task_id = format!("model:{engine}");
    let token = state
        .model_cancel
        .try_create(&task_id)
        .ok_or_else(|| "Эта модель уже подготавливается".to_string())?;
    let _task_guard = CancelRegistryGuard::new(state.model_cancel.clone(), task_id);
    download_model_inner(app, &engine, token).await
}

// ponytail: отмена во время install_runtime срабатывает только на следующем
// чекпоинте токена — задержка соответствует существующему паттерну загрузки.
#[tauri::command]
pub(crate) fn cancel_model_prepare(engine: String, state: State<'_, AppState>) -> bool {
    let task_id = if engine == "summary" {
        "model:summary".to_string()
    } else {
        format!("model:{engine}")
    };
    state.model_cancel.cancel(&task_id)
}

async fn download_model_inner(
    app: AppHandle,
    engine: &str,
    token: std::sync::Arc<cancellation::CancelToken>,
) -> Result<(), String> {
    match engine {
        "parakeet" => {
            let dir = paths::parakeet_dir(&app).map_err(|e| e.to_string())?;
            model::download_parakeet(app.clone(), &dir, &token)
                .await
                .map_err(|e| e.to_string())?;
            transcriber_parakeet::spawn_mlx_install(app.clone());
        }
        "whisper" => {
            let main = paths::model_path(&app).map_err(|e| e.to_string())?;
            let coreml = paths::coreml_encoder_path(&app).map_err(|e| e.to_string())?;
            model::download_whisper(app.clone(), &main, &coreml, &token)
                .await
                .map_err(|e| e.to_string())?;
        }
        engine if transcriber_qwen::is_qwen_engine(engine) => {
            if let Some(reason) = transcriber_qwen::unavailable_reason() {
                return Err(reason);
            }
            let _ = app.emit("model:progress", 0u32);
            let _ = app.emit("model:stage", "installing");
            let app_for_install = app.clone();
            tauri::async_runtime::spawn_blocking(move || {
                transcriber_qwen::install_runtime(&app_for_install, |line| {
                    tracing::info!("Qwen runtime install: {line}");
                })
            })
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
            if token.is_cancelled() {
                return Err("cancelled".to_string());
            }

            let expected_bytes = transcriber_qwen::expected_bytes_for_engine(engine)
                .ok_or_else(|| format!("Неизвестная Qwen-модель: {engine}"))?;
            let _ = app.emit("model:progress", 1u32);
            let _ = app.emit("model:stage", "downloading");
            let engine_str = engine.to_string();
            let app_for_task = app.clone();
            let cache_dir =
                transcriber_qwen::model_cache_dir(&app, engine).map_err(|e| e.to_string())?;

            let poll_handle = spawn_model_progress_poller(
                app.clone(),
                cache_dir,
                expected_bytes,
                "model:progress",
                "model:stage",
            );

            let result = tauri::async_runtime::spawn_blocking(move || {
                transcriber_qwen::warmup_model(&app_for_task, &engine_str, token)
            })
            .await;
            poll_handle.abort();
            result
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?;
            let _ = app.emit("model:stage", "ready");
            let _ = app.emit("model:progress", 100u32);
        }
        other => return Err(format!("Неизвестный движок транскрибации: {other}")),
    }
    preload_active_engine(app);
    Ok(())
}

#[tauri::command]
pub(crate) async fn delete_model_for_engine(
    app: AppHandle,
    state: State<'_, AppState>,
    engine: String,
) -> Result<(), String> {
    if state.queue.has_active_jobs() {
        return Err(
            "Нельзя удалять модель во время активной транскрибации. Дождитесь окончания или отмените задачу."
                .to_string(),
        );
    }
    tauri::async_runtime::spawn_blocking(move || delete_model_files(&app, &engine))
        .await
        .map_err(|e| e.to_string())?
}

fn delete_model_files(app: &AppHandle, engine: &str) -> Result<(), String> {
    match engine {
        "parakeet" => {
            transcriber_parakeet::clear_cache();
            let dir = paths::parakeet_dir(app).map_err(|e| e.to_string())?;
            remove_path_if_exists(&dir).map_err(|e| e.to_string())?;
        }
        "whisper" => {
            transcriber::clear_cache();
            let main = paths::model_path(app).map_err(|e| e.to_string())?;
            let coreml = paths::coreml_encoder_path(app).map_err(|e| e.to_string())?;
            remove_path_if_exists(&main).map_err(|e| e.to_string())?;
            remove_path_if_exists(&coreml).map_err(|e| e.to_string())?;

            if let Ok(models_dir) = paths::app_data_dir(app).map(|p| p.join("models")) {
                let _ = remove_path_if_exists(&models_dir.join("__MACOSX"));
            }
        }
        engine if transcriber_qwen::is_qwen_engine(engine) => {
            transcriber_qwen::stop_server();
            transcriber_qwen::clear_ready_marker(app, engine);
            let model = transcriber_qwen::model_for_engine(engine)
                .ok_or_else(|| format!("Неизвестная Qwen-модель: {engine}"))?;
            let cache_dir = paths::qwen_cache_dir(app).map_err(|e| e.to_string())?;
            let repo_cache_name = format!("models--{}", model.replace('/', "--"));
            remove_path_if_exists(&cache_dir.join("hub").join(repo_cache_name))
                .map_err(|e| e.to_string())?;
        }
        other => return Err(format!("Неизвестный движок транскрибации: {other}")),
    }
    Ok(())
}

fn remove_path_if_exists(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        std::fs::remove_dir_all(path)?;
    } else if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}
