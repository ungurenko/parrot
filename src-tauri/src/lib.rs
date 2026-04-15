mod binaries;
mod cancellation;
mod model;
mod paths;
mod queue;
mod settings;
mod source;
mod transcriber;
mod transcriber_parakeet;
mod transcriber_qwen;
mod writer;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use queue::{Job, JobQueue, SourceKind};
use settings::Settings;

pub struct AppState {
    queue: Arc<JobQueue>,
}

#[derive(Serialize)]
struct EnqueueResult {
    id: String,
}

const SUPPORTED_ENGINES: [&str; 4] = [
    "parakeet",
    "qwen-0.6b",
    "qwen-1.7b",
    "whisper",
];

#[tauri::command]
async fn enqueue_file(
    path: String,
    state: State<'_, AppState>,
) -> Result<EnqueueResult, String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("Файл не найден: {path}"));
    }
    if !source::is_supported_file(&p) {
        return Err("Неподдерживаемый формат файла".into());
    }
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
        })
        .await
        .map_err(|e| e.to_string())?;
    Ok(EnqueueResult { id })
}

#[tauri::command]
async fn enqueue_youtube(
    url: String,
    state: State<'_, AppState>,
) -> Result<EnqueueResult, String> {
    let trimmed = url.trim().to_string();
    if !source::is_youtube_url(&trimmed) {
        return Err("URL не похож на YouTube-ссылку".into());
    }
    let id = queue::new_job_id();
    state
        .queue
        .enqueue(Job {
            id: id.clone(),
            source: SourceKind::YouTube(trimmed.clone()),
            display_name: trimmed,
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
    settings::save(&app, &new).map_err(|e| e.to_string()).map(|_| {
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
fn get_model_statuses(app: AppHandle) -> HashMap<String, bool> {
    SUPPORTED_ENGINES
        .into_iter()
        .map(|engine| (engine.to_string(), is_model_ready_for_engine(&app, engine)))
        .collect()
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
                        let started = warmup_started
                            .get_or_insert_with(std::time::Instant::now);
                        if stage_emitted != "warmup" {
                            let _ = poll_app.emit("model:stage", "warmup");
                            stage_emitted = "warmup";
                        }
                        let elapsed = started.elapsed().as_secs_f64();
                        // 95 → 99 over ~20 sec of warmup.
                        let pct = (95.0 + (elapsed / 20.0).min(1.0) * 4.0) as u32;
                        let _ = poll_app.emit("model:progress", pct);
                    } else {
                        let pct = ((size as f64 / expected_bytes as f64) * 95.0)
                            .clamp(1.0, 95.0) as u32;
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
fn read_text_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
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
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let handle = app.handle().clone();
            if let Err(e) = paths::migrate_legacy_app_data(&handle) {
                eprintln!("Legacy app data migration failed: {e:#}");
            }
            if let Some(win) = app.get_webview_window("main") {
                win.on_window_event(|event| {
                    if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                        binaries::stop_yt_dlp_startup_cache();
                        transcriber_qwen::stop_server();
                    }
                });
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
            app.manage(AppState { queue });

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
            get_model_statuses,
            download_model,
            download_model_for_engine,
            delete_model,
            delete_model_for_engine,
            cancel_job,
            read_text_file,
            open_in_finder,
            open_logs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
