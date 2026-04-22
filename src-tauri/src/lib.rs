mod binaries;
mod cancellation;
mod history;
mod model;
mod paths;
mod prompts;
mod queue;
mod settings;
mod source;
mod summarizer_qwen3;
mod transcriber;
mod transcriber_parakeet;
mod transcriber_qwen;
mod writer;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use cancellation::CancelRegistry;
use queue::{Job, JobQueue, SourceKind};
use settings::Settings;

pub struct AppState {
    queue: Arc<JobQueue>,
    summary_cancel: CancelRegistry,
}

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

#[derive(Serialize)]
struct EnqueueResult {
    id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct EngineStatus {
    available: bool,
    model_ready: bool,
    unavailable_reason: Option<String>,
}

#[tauri::command]
async fn enqueue_file(path: String, state: State<'_, AppState>) -> Result<EnqueueResult, String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("Файл не найден: {path}"));
    }
    if !source::is_supported_file(&p) {
        return Err("Неподдерживаемый формат файла".into());
    }
    let settings = settings::load(&state.queue.app_handle());
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
            engine: settings.engine,
            language: settings.language,
            save_dir: settings.save_dir,
        })
        .await
        .map_err(|e| e.to_string())?;
    Ok(EnqueueResult { id })
}

#[tauri::command]
async fn enqueue_youtube(url: String, state: State<'_, AppState>) -> Result<EnqueueResult, String> {
    let trimmed = url.trim().to_string();
    if !source::is_youtube_url(&trimmed) {
        return Err("URL не похож на YouTube-ссылку".into());
    }
    let settings = settings::load(&state.queue.app_handle());
    let id = queue::new_job_id();
    state
        .queue
        .enqueue(Job {
            id: id.clone(),
            source: SourceKind::YouTube(trimmed.clone()),
            display_name: trimmed,
            engine: settings.engine,
            language: settings.language,
            save_dir: settings.save_dir,
        })
        .await
        .map_err(|e| e.to_string())?;
    Ok(EnqueueResult { id })
}

#[tauri::command]
fn get_settings(app: AppHandle) -> Settings {
    settings::load(&app)
}

#[tauri::command]
fn set_settings(app: AppHandle, new: Settings) -> Result<(), String> {
    let old = settings::load(&app);
    settings::save(&app, &new)
        .map_err(|e| e.to_string())
        .map(|_| {
            if old.engine != new.engine {
                preload_active_engine(app);
            }
        })
}

/// Whether the model for the currently-selected engine is downloaded and ready.
#[tauri::command]
fn is_model_ready(app: AppHandle) -> bool {
    let s = settings::load(&app);
    is_model_ready_for_engine(&app, &s.engine)
}

#[tauri::command]
fn get_engine_statuses(app: AppHandle) -> HashMap<String, EngineStatus> {
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

fn is_model_ready_for_engine(app: &AppHandle, engine: &str) -> bool {
    match engine {
        "parakeet" => paths::parakeet_files_ready(app),
        "whisper" => {
            let main = paths::model_path(app).map(|p| p.exists()).unwrap_or(false);
            let coreml = paths::coreml_encoder_path(app)
                .map(|p| p.exists())
                .unwrap_or(false);
            main && coreml
        }
        engine if transcriber_qwen::is_qwen_engine(engine) => {
            transcriber_qwen::is_ready(app, engine)
        }
        _ => false,
    }
}

#[tauri::command]
async fn download_model(app: AppHandle) -> Result<(), String> {
    let engine = settings::load(&app).engine;
    download_model_for_engine(app, engine).await
}

#[tauri::command]
async fn download_model_for_engine(app: AppHandle, engine: String) -> Result<(), String> {
    download_model_inner(app, &engine).await
}

async fn download_model_inner(app: AppHandle, engine: &str) -> Result<(), String> {
    match engine {
        "parakeet" => {
            let dir = paths::parakeet_dir(&app).map_err(|e| e.to_string())?;
            model::download_parakeet(app.clone(), &dir)
                .await
                .map_err(|e| e.to_string())?;
        }
        "whisper" => {
            let main = paths::model_path(&app).map_err(|e| e.to_string())?;
            let coreml = paths::coreml_encoder_path(&app).map_err(|e| e.to_string())?;
            model::download_whisper(app.clone(), &main, &coreml)
                .await
                .map_err(|e| e.to_string())?;
        }
        engine if transcriber_qwen::is_qwen_engine(engine) => {
            let expected_bytes: u64 = match engine {
                transcriber_qwen::ENGINE_QWEN_0_6B => transcriber_qwen::EXPECTED_QWEN_0_6B_BYTES,
                transcriber_qwen::ENGINE_QWEN_1_7B => transcriber_qwen::EXPECTED_QWEN_1_7B_BYTES,
                _ => transcriber_qwen::EXPECTED_QWEN_1_7B_BYTES,
            };
            let _ = app.emit("model:progress", 1u32);
            let _ = app.emit("model:stage", "downloading");
            let engine_str = engine.to_string();
            let app_for_task = app.clone();
            let cache_dir = paths::qwen_cache_dir(&app).map_err(|e| e.to_string())?;

            let poll_app = app.clone();
            let poll_handle = tauri::async_runtime::spawn(async move {
                let warmup_threshold = (expected_bytes as f64 * 0.9) as u64;
                let mut warmup_started: Option<std::time::Instant> = None;
                let mut stage_emitted = "downloading";
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                    let size = dir_size_bytes(&cache_dir);
                    if size >= warmup_threshold {
                        let started = warmup_started.get_or_insert_with(std::time::Instant::now);
                        if stage_emitted != "warmup" {
                            let _ = poll_app.emit("model:stage", "warmup");
                            stage_emitted = "warmup";
                        }
                        let elapsed = started.elapsed().as_secs_f64();
                        // 95 → 99 over ~20 sec of warmup.
                        let pct = (95.0 + (elapsed / 20.0).min(1.0) * 4.0) as u32;
                        let _ = poll_app.emit("model:progress", pct);
                    } else {
                        let pct =
                            ((size as f64 / expected_bytes as f64) * 95.0).clamp(1.0, 95.0) as u32;
                        let _ = poll_app.emit("model:progress", pct);
                    }
                }
            });

            let result = tauri::async_runtime::spawn_blocking(move || {
                transcriber_qwen::warmup_model(&app_for_task, &engine_str)
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
async fn delete_model(app: AppHandle) -> Result<(), String> {
    let engine = settings::load(&app).engine;
    delete_model_for_engine(app, engine).await
}

#[tauri::command]
async fn delete_model_for_engine(app: AppHandle, engine: String) -> Result<(), String> {
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

#[tauri::command]
fn cancel_job(id: String, state: State<'_, AppState>) -> bool {
    state.queue.cancel_registry().cancel(&id)
}

#[tauri::command]
fn get_history(app: AppHandle) -> Vec<history::HistoryEntry> {
    history::load(&app)
}

#[tauri::command]
fn delete_history_entry(id: String, app: AppHandle) -> Result<(), String> {
    history::remove(&app, &id).map_err(|e| e.to_string())?;
    let _ = app.emit("history:updated", ());
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LoadedHistoryEntry {
    entry: history::HistoryEntry,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
}

#[tauri::command]
fn load_history_entry(id: String, app: AppHandle) -> Result<LoadedHistoryEntry, String> {
    let entry = history::get(&app, &id).ok_or_else(|| "Запись не найдена".to_string())?;
    let text = std::fs::read_to_string(&entry.output_path)
        .map_err(|e| format!("Не удалось прочитать транскрипт: {e}"))?;
    let summary = entry
        .summary_path
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok());
    Ok(LoadedHistoryEntry {
        entry,
        text,
        summary,
    })
}

#[tauri::command]
fn get_summarizer_status(app: AppHandle) -> summarizer_qwen3::SummarizerStatus {
    summarizer_qwen3::status(&app)
}

#[tauri::command]
async fn download_summarizer_model(app: AppHandle) -> Result<(), String> {
    let _ = app.emit("summary_model:progress", 1u32);
    let _ = app.emit("summary_model:stage", "downloading");

    let expected_bytes = summarizer_qwen3::EXPECTED_SUMMARY_BYTES;
    let cache_dir = paths::qwen_cache_dir(&app).map_err(|e| e.to_string())?;
    let poll_app = app.clone();
    let poll_handle = tauri::async_runtime::spawn(async move {
        let warmup_threshold = (expected_bytes as f64 * 0.9) as u64;
        let mut warmup_started: Option<std::time::Instant> = None;
        let mut stage_emitted = "downloading";
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            let size = dir_size_bytes(&cache_dir);
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
    let result =
        tauri::async_runtime::spawn_blocking(move || summarizer_qwen3::warmup_model(&app_for_task))
            .await;
    poll_handle.abort();
    result
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let _ = app.emit("summary_model:stage", "ready");
    let _ = app.emit("summary_model:progress", 100u32);
    Ok(())
}

#[tauri::command]
async fn delete_summarizer_model(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || summarizer_qwen3::delete_model(&app))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn cancel_summary(id: String, state: State<'_, AppState>) -> bool {
    state.summary_cancel.cancel(&id)
}

#[tauri::command]
async fn summarize(
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

    // Validate transcript_path: must be a .txt file inside save_dir (hardening
    // against a frontend or IPC caller pointing at an arbitrary filesystem path).
    let transcript_path_buf = PathBuf::from(&transcript_path);
    if transcript_path_buf
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.eq_ignore_ascii_case("txt"))
        != Some(true)
    {
        return Err("Путь транскрипта должен указывать на .txt".to_string());
    }
    let save_dir = settings::load(&app).save_dir;
    let canonical_save = save_dir.canonicalize().unwrap_or(save_dir);
    let canonical_transcript = transcript_path_buf
        .canonicalize()
        .map_err(|e| format!("Файл транскрипта недоступен: {e}"))?;
    if !canonical_transcript.starts_with(&canonical_save) {
        return Err("Транскрипт должен находиться в папке сохранения".to_string());
    }

    // Atomic create: refuses to start a second concurrent summarize for the same
    // job id (protects against UI races where a previous run is still winding down).
    let token = state
        .summary_cancel
        .try_create(&id)
        .ok_or_else(|| "Конспект для этой записи уже генерируется".to_string())?;
    let cancel_registry = state.summary_cancel.clone();

    let transcript_path_buf = canonical_transcript;
    let id_for_task = id.clone();
    let id_for_progress = id.clone();
    let app_for_task = app.clone();
    let app_for_progress = app.clone();
    let transcript_len = transcript.chars().count();

    // Progress ticker: 0→95 over estimated duration.
    // Rough heuristic: 4s model load + (expected output tokens / 60 tok/s).
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

#[tauri::command]
fn open_in_finder(path: String) -> Result<(), String> {
    std::process::Command::new("open")
        .arg("-R")
        .arg(&path)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn open_logs(app: AppHandle) -> Result<(), String> {
    let dir = paths::logs_dir(&app).map_err(|e| e.to_string())?;
    std::process::Command::new("open")
        .arg(dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn dir_size_bytes(path: &std::path::Path) -> u64 {
    let mut total: u64 = 0;
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() {
                total = total.saturating_add(meta.len());
            } else if meta.is_dir() {
                total = total.saturating_add(dir_size_bytes(&p));
            }
        }
    }
    total
}

fn preload_active_engine(handle: AppHandle) {
    tauri::async_runtime::spawn_blocking(move || {
        let s = settings::load(&handle);
        match s.engine.as_str() {
            "parakeet" => {
                transcriber_qwen::stop_server();
                if paths::parakeet_files_ready(&handle) {
                    if let Ok(dir) = paths::parakeet_dir(&handle) {
                        tracing::info!("Preloading Parakeet TDT v3…");
                        match transcriber_parakeet::preload(&dir) {
                            Ok(_) => tracing::info!("Parakeet preload complete"),
                            Err(e) => tracing::error!("Parakeet preload failed: {e:#}"),
                        }
                    }
                }
            }
            "whisper" => {
                transcriber_qwen::stop_server();
                let main_ok = paths::model_path(&handle)
                    .map(|p| p.exists())
                    .unwrap_or(false);
                let coreml_ok = paths::coreml_encoder_path(&handle)
                    .map(|p| p.exists())
                    .unwrap_or(false);
                if main_ok && coreml_ok {
                    if let Ok(p) = paths::model_path(&handle) {
                        tracing::info!("Preloading Whisper + Core ML…");
                        match transcriber::preload(&p) {
                            Ok(_) => tracing::info!("Whisper preload complete"),
                            Err(e) => tracing::error!("Whisper preload failed: {e:#}"),
                        }
                    }
                }
            }
            engine if transcriber_qwen::is_qwen_engine(engine) => {
                if transcriber_qwen::is_ready(&handle, engine) {
                    tracing::info!("Preloading Qwen MLX warm server…");
                    match transcriber_qwen::preload_server(&handle, engine) {
                        Ok(_) => tracing::info!("Qwen MLX warm server ready"),
                        Err(e) => tracing::error!("Qwen MLX warm server failed: {e:#}"),
                    }
                } else {
                    tracing::info!("Qwen MLX selected; model is not ready yet");
                }
            }
            other => {
                tracing::warn!("Unknown transcription engine selected: {other}");
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let handle = app.handle().clone();
            if let Err(e) = paths::migrate_legacy_app_data(&handle) {
                eprintln!("Legacy app data migration failed: {e:#}");
            }
            if let Ok(log_dir) = paths::logs_dir(&handle) {
                let file_appender = tracing_appender::rolling::daily(log_dir, "app.log");
                let (nb, guard) = tracing_appender::non_blocking(file_appender);
                Box::leak(Box::new(guard));
                let _ = tracing_subscriber::fmt()
                    .with_writer(nb)
                    .with_ansi(false)
                    .with_env_filter(
                        tracing_subscriber::EnvFilter::try_from_default_env()
                            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
                    )
                    .try_init();
            }

            paths::cleanup_tmp(&handle);

            let s = settings::load(&handle);
            let _ = std::fs::create_dir_all(&s.save_dir);

            let queue = JobQueue::start(handle.clone());
            let summary_cancel = CancelRegistry::new();
            let summary_cancel_for_close = summary_cancel.clone();
            app.manage(AppState {
                queue,
                summary_cancel,
            });

            if let Some(win) = app.get_webview_window("main") {
                win.on_window_event(move |event| {
                    if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                        binaries::stop_yt_dlp_startup_cache();
                        transcriber_qwen::stop_server();
                        // Kill any running mlx-lm summarization subprocess so we
                        // don't leave a zombie Python process after window close.
                        summary_cancel_for_close.cancel_all();
                    }
                });
            }

            preload_active_engine(handle.clone());
            binaries::warm_yt_dlp_startup_cache(handle.clone());

            tracing::info!("Parrot started (engine: {})", s.engine);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            enqueue_file,
            enqueue_youtube,
            get_settings,
            set_settings,
            is_model_ready,
            get_engine_statuses,
            download_model,
            download_model_for_engine,
            delete_model,
            delete_model_for_engine,
            cancel_job,
            open_in_finder,
            open_logs,
            get_summarizer_status,
            download_summarizer_model,
            delete_summarizer_model,
            summarize,
            cancel_summary,
            get_history,
            delete_history_entry,
            load_history_entry,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
