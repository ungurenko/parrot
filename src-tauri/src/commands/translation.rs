use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::{history, summarizer_qwen3, translation, writer};
use crate::{validate_saved_file_path, AppState, SavedFileKind};

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TranslationProgressEvent {
    id: String,
    percent: u32,
    stage: String,
    current_part: usize,
    total_parts: usize,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TranslationDoneEvent {
    id: String,
    text: String,
    output_path: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TranslationErrorEvent {
    id: String,
    message: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TranslationCanceledEvent {
    id: String,
}

#[tauri::command]
pub(crate) fn cancel_translation(id: String, state: State<'_, AppState>) -> bool {
    state.local_llm_tasks.cancel(&format!("translation:{id}"))
}

#[tauri::command]
pub(crate) async fn translate_to_russian(
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
        return Err("Локальная модель не готова. Откройте настройки и подготовьте её.".to_string());
    }

    let transcript_path_buf =
        validate_saved_file_path(&app, &transcript_path, SavedFileKind::Transcript)?;
    let task_id = format!("translation:{id}");
    let generation_lease = state
        .local_llm_tasks
        .try_start(&task_id)
        .ok_or_else(|| "Локальная модель уже занята другой задачей".to_string())?;
    let token = generation_lease.token();

    let id_for_task = id.clone();
    let app_for_task = app.clone();
    let token_for_task = token.clone();
    let result =
        tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<(String, PathBuf)> {
            let app_for_progress = app_for_task.clone();
            let progress_id = id_for_task.clone();
            let translation = {
                let mut generation_lease = generation_lease;
                let translation = translation::translate_text_with(
                    &transcript,
                    translation::TRANSLATION_CHUNK_CHARS,
                    token_for_task.as_ref(),
                    |chunk, part, total| {
                        summarizer_qwen3::generate_translation_chunk(
                            &app_for_task,
                            chunk,
                            part,
                            total,
                            token_for_task.clone(),
                        )
                    },
                    |completed, total| {
                        let (stage, percent, current_part) = if completed == 0 {
                            ("loading", 2, 1)
                        } else {
                            let percent = ((completed as f64 / total as f64) * 95.0).round() as u32;
                            ("translating", percent, (completed + 1).min(total))
                        };
                        let _ = app_for_progress.emit(
                            "translation:progress",
                            TranslationProgressEvent {
                                id: progress_id.clone(),
                                percent,
                                stage: stage.to_string(),
                                current_part,
                                total_parts: total,
                            },
                        );
                    },
                )?;
                if token_for_task.is_cancelled() {
                    anyhow::bail!("cancelled");
                }
                if !generation_lease.begin_commit() {
                    anyhow::bail!("cancelled");
                }
                translation
            };
            let total_parts = translation::split_translation_chunks(
                &transcript,
                translation::TRANSLATION_CHUNK_CHARS,
            )
            .len();
            let _ = app_for_task.emit(
                "translation:progress",
                TranslationProgressEvent {
                    id: id_for_task.clone(),
                    percent: 98,
                    stage: "saving".to_string(),
                    current_part: total_parts,
                    total_parts,
                },
            );
            let output = writer::save_translation(
                &transcript_path_buf,
                &translation,
                token_for_task.as_ref(),
            )?;
            Ok((translation, output))
        })
        .await;

    match result {
        Ok(Ok((text, output))) => {
            if let Err(error) = history::attach_translation(&app, &id, &output) {
                tracing::warn!("history attach_translation failed for {id}: {error:#}");
            } else {
                let _ = app.emit("history:updated", ());
            }
            let _ = app.emit(
                "translation:done",
                TranslationDoneEvent {
                    id,
                    text,
                    output_path: output.to_string_lossy().to_string(),
                },
            );
            Ok(())
        }
        Ok(Err(error)) => {
            let message = format!("{error:#}");
            if message == "cancelled" || token.is_cancelled() {
                let _ = app.emit("translation:canceled", TranslationCanceledEvent { id });
                Ok(())
            } else {
                tracing::error!("translation {id} failed: {message}");
                let _ = app.emit(
                    "translation:error",
                    TranslationErrorEvent {
                        id,
                        message: message.clone(),
                    },
                );
                Err(message)
            }
        }
        Err(error) => {
            let message = error.to_string();
            tracing::error!("translation {id} spawn failed: {message}");
            let _ = app.emit(
                "translation:error",
                TranslationErrorEvent {
                    id,
                    message: message.clone(),
                },
            );
            Err(message)
        }
    }
}
