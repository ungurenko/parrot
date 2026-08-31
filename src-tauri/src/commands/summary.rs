use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

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
pub(crate) async fn setup_summarizer_env(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let _local_task = state
        .local_llm_tasks
        .try_start("model:setup")
        .ok_or_else(local_model_busy_message)?;
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
    let local_tasks = state.local_llm_tasks.clone();
    let local_task = local_tasks
        .try_start("model:download")
        .ok_or_else(local_model_busy_message)?;
    let task_id = "model:summary".to_string();
    let token = state
        .model_cancel
        .try_create(&task_id)
        .ok_or_else(|| "Модель конспекта уже подготавливается".to_string())?;
    let _task_guard = CancelRegistryGuard::new(state.model_cancel.clone(), task_id);
    let expected_bytes = summarizer_qwen3::expected_summary_bytes(&app);
    let cache_dir = paths::qwen_cache_dir(&app).map_err(|e| e.to_string())?;
    let app_for_task = app.clone();
    let token_for_task = token.clone();
    super::run_model_warmup(
        app.clone(),
        cache_dir,
        expected_bytes,
        "summary_model:progress",
        "summary_model:stage",
        move || summarizer_qwen3::warmup_model(&app_for_task, token_for_task),
    )
    .await?;
    drop(local_task);
    summarizer_qwen3::preload_server(app.clone(), local_tasks);
    Ok(())
}

#[tauri::command]
pub(crate) async fn delete_summarizer_model(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let _local_task = state
        .local_llm_tasks
        .try_start("model:delete")
        .ok_or_else(local_model_busy_message)?;
    tauri::async_runtime::spawn_blocking(move || summarizer_qwen3::delete_model(&app))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

fn local_model_busy_message() -> String {
    "Локальная модель занята. Дождитесь завершения текущей задачи.".to_string()
}

#[tauri::command]
pub(crate) fn cancel_summary(id: String, state: State<'_, AppState>) -> bool {
    state.local_llm_tasks.cancel(&format!("summary:{id}"))
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

    let task_id = format!("summary:{id}");
    let generation_lease = state
        .local_llm_tasks
        .try_start(&task_id)
        .ok_or_else(|| "Локальная модель уже занята другой задачей".to_string())?;
    let token = generation_lease.token();

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
            let summary = {
                let mut generation_lease = generation_lease;
                let summary = summarizer_qwen3::generate_summary(
                    &app_for_task,
                    &transcript_owned,
                    token_for_task.clone(),
                )?;
                if token_for_task.is_cancelled() || !generation_lease.begin_commit() {
                    anyhow::bail!("cancelled");
                }
                summary
            };
            let output = writer::save_summary(&transcript_path_buf, &summary)?;
            Ok((summary, output))
        })
        .await;

    ticker_stop.store(true, std::sync::atomic::Ordering::Relaxed);
    ticker_handle.abort();

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
