use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::fs_metrics::DirSizeCache;
use crate::{history, paths, summarizer_qwen3, writer};
use crate::{validate_saved_file_path, AppState, CancelRegistryGuard, SavedFileKind};

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SummaryProgressEvent {
    id: String,
    percent: u32,
    stage: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SummaryDoneEvent {
    id: String,
    markdown: String,
    output_path: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SummaryErrorEvent {
    id: String,
    message: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SummaryCanceledEvent {
    id: String,
}

#[tauri::command]
pub(crate) fn get_summarizer_status(app: AppHandle) -> summarizer_qwen3::SummarizerStatus {
    summarizer_qwen3::status(&app)
}

#[tauri::command]
pub(crate) async fn setup_summarizer_env(app: AppHandle) -> Result<(), String> {
    let app_for_task = app.clone();
    let app_for_log = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        summarizer_qwen3::install_env(&app_for_task, |line| {
            let _ = app_for_log.emit("summary_env:progress", line.to_string());
        })
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn download_summarizer_model(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let task_id = "model:summary".to_string();
    let token = state
        .model_cancel
        .try_create(&task_id)
        .ok_or_else(|| "Модель конспекта уже подготавливается".to_string())?;
    let _task_guard = CancelRegistryGuard::new(state.model_cancel.clone(), task_id);
    let _ = app.emit("summary_model:progress", 1u32);
    let _ = app.emit("summary_model:stage", "downloading");

    let expected_bytes = summarizer_qwen3::EXPECTED_SUMMARY_BYTES;
    let cache_dir = paths::qwen_cache_dir(&app).map_err(|e| e.to_string())?;
    let poll_app = app.clone();
    let poll_handle = tauri::async_runtime::spawn(async move {
        let mut size_cache = DirSizeCache::default();
        let warmup_threshold = (expected_bytes as f64 * 0.9) as u64;
        let mut warmup_started: Option<std::time::Instant> = None;
        let mut stage_emitted = "downloading";
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            size_cache.clear();
            let size = size_cache.get(&cache_dir);
            if size >= warmup_threshold {
                let started = warmup_started.get_or_insert_with(std::time::Instant::now);
                if stage_emitted != "warmup" {
                    let _ = poll_app.emit("summary_model:stage", "warmup");
                    stage_emitted = "warmup";
                }
                let elapsed = started.elapsed().as_secs_f64();
                let pct = (95.0 + (elapsed / 20.0).min(1.0) * 4.0) as u32;
                let _ = poll_app.emit("summary_model:progress", pct);
            } else {
                let pct = ((size as f64 / expected_bytes as f64) * 95.0).clamp(1.0, 95.0) as u32;
                let _ = poll_app.emit("summary_model:progress", pct);
            }
        }
    });

    let app_for_task = app.clone();
    let token_for_task = token.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        summarizer_qwen3::warmup_model(&app_for_task, token_for_task)
    })
    .await;
    poll_handle.abort();
    let result = match result {
        Ok(inner) => inner.map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    };
    result?;
    let _ = app.emit("summary_model:stage", "ready");
    let _ = app.emit("summary_model:progress", 100u32);
    Ok(())
}

#[tauri::command]
pub(crate) async fn delete_summarizer_model(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || summarizer_qwen3::delete_model(&app))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn cancel_summary(id: String, state: State<'_, AppState>) -> bool {
    state.summary_cancel.cancel(&id)
}

#[tauri::command]
pub(crate) async fn summarize(
    id: String,
    transcript: String,
    transcript_path: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if transcript.trim().is_empty() {
        return Err("Пустой транскрипт.".to_string());
    }
    if !summarizer_qwen3::is_ready(&app) {
        return Err("Модель конспекта не готова. Откройте настройки и подготовьте её.".to_string());
    }

    let transcript_path_buf =
        validate_saved_file_path(&app, &transcript_path, SavedFileKind::Transcript)?;

    let token = state
        .summary_cancel
        .try_create(&id)
        .ok_or_else(|| "Конспект для этой записи уже генерируется".to_string())?;
    let cancel_registry = state.summary_cancel.clone();

    let id_for_task = id.clone();
    let id_for_progress = id.clone();
    let app_for_task = app.clone();
    let app_for_progress = app.clone();
    let transcript_len = transcript.chars().count();

    let expected_output_tokens = ((transcript_len as f64) / 40.0).clamp(200.0, 2000.0);
    let expected_seconds = 4.0 + expected_output_tokens / 60.0;
    let ticker_stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let ticker_flag = ticker_stop.clone();
    let ticker_handle = tauri::async_runtime::spawn(async move {
        let started = std::time::Instant::now();
        loop {
            if ticker_flag.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            let elapsed = started.elapsed().as_secs_f64();
            let stage = if elapsed < 4.0 {
                "loading"
            } else {
                "generating"
            };
            let ratio = (elapsed / expected_seconds).min(1.0);
            let pct = (ratio * 95.0).round() as u32;
            let _ = app_for_progress.emit(
                "summary:progress",
                SummaryProgressEvent {
                    id: id_for_progress.clone(),
                    percent: pct.max(2),
                    stage: stage.to_string(),
                },
            );
            tokio::time::sleep(std::time::Duration::from_millis(700)).await;
        }
    });

    let transcript_owned = transcript.clone();
    let token_for_task = token.clone();
    let result =
        tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<(String, PathBuf)> {
            let summary = summarizer_qwen3::generate_summary(
                &app_for_task,
                &transcript_owned,
                token_for_task,
            )?;
            let output = writer::save_summary(&transcript_path_buf, &summary)?;
            Ok((summary, output))
        })
        .await;

    ticker_stop.store(true, std::sync::atomic::Ordering::Relaxed);
    ticker_handle.abort();
    cancel_registry.remove(&id_for_task);

    match result {
        Ok(Ok((markdown, output))) => {
            if let Err(e) = history::attach_summary(&app, &id_for_task, &output) {
                tracing::warn!("history attach_summary failed for {id_for_task}: {e:#}",);
            } else {
                let _ = app.emit("history:updated", ());
            }
            let _ = app.emit(
                "summary:progress",
                SummaryProgressEvent {
                    id: id_for_task.clone(),
                    percent: 100,
                    stage: "finalizing".to_string(),
                },
            );
            let _ = app.emit(
                "summary:done",
                SummaryDoneEvent {
                    id: id_for_task,
                    markdown,
                    output_path: output.to_string_lossy().to_string(),
                },
            );
            Ok(())
        }
        Ok(Err(e)) => {
            let msg = format!("{e:#}");
            if msg == "cancelled" || token.is_cancelled() {
                let _ = app.emit("summary:canceled", SummaryCanceledEvent { id: id_for_task });
            } else {
                tracing::error!("summary {} failed: {msg}", id_for_task);
                let _ = app.emit(
                    "summary:error",
                    SummaryErrorEvent {
                        id: id_for_task,
                        message: msg.clone(),
                    },
                );
                return Err(msg);
            }
            Ok(())
        }
        Err(e) => {
            let msg = e.to_string();
            tracing::error!("summary {} spawn failed: {msg}", id_for_task);
            let _ = app.emit(
                "summary:error",
                SummaryErrorEvent {
                    id: id_for_task,
                    message: msg.clone(),
                },
            );
            Err(msg)
        }
    }
}
