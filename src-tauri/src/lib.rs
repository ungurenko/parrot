mod binaries;
mod cancellation;
mod commands;
mod dictation;
mod fs_metrics;
mod history;
mod model;
mod paths;
mod prompts;
mod queue;
mod settings;
mod source;
mod summarizer_models;
mod summarizer_qwen3;
mod transcriber;
mod transcriber_parakeet;
mod transcriber_qwen;
mod writer;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use tauri::{AppHandle, Manager, State};

use cancellation::CancelRegistry;
use queue::JobQueue;
use settings::Settings;

pub struct AppState {
    pub(crate) queue: Arc<JobQueue>,
    pub(crate) summary_cancel: CancelRegistry,
    pub(crate) model_cancel: CancelRegistry,
}

pub(crate) struct CancelRegistryGuard {
    registry: CancelRegistry,
    id: String,
}

impl CancelRegistryGuard {
    pub(crate) fn new(registry: CancelRegistry, id: String) -> Self {
        Self { registry, id }
    }
}

impl Drop for CancelRegistryGuard {
    fn drop(&mut self) {
        self.registry.remove(&self.id);
    }
}

pub(crate) static PRELOAD_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[tauri::command]
fn get_settings(app: AppHandle) -> Settings {
    settings::load(&app)
}

#[tauri::command]
fn get_dictation_status(
    app: AppHandle,
    dictation: State<'_, dictation::DictationManager>,
) -> dictation::DictationStatus {
    dictation.status(&app)
}

#[tauri::command]
fn check_accessibility_permission() -> bool {
    dictation::accessibility_permission_granted()
}

#[tauri::command]
fn request_accessibility_permission() -> bool {
    dictation::request_accessibility_permission()
}

#[tauri::command]
fn set_settings(
    app: AppHandle,
    dictation: State<'_, dictation::DictationManager>,
    state: State<'_, AppState>,
    new: Settings,
) -> Result<(), String> {
    let old = settings::load(&app);
    if old.engine != new.engine && state.queue.has_active_jobs() {
        return Err(
            "Нельзя менять движок во время активной транскрибации. Дождитесь окончания или отмените задачу."
                .to_string(),
        );
    }
    if old.dictation_enabled != new.dictation_enabled
        || old.dictation_hold_key != new.dictation_hold_key
    {
        dictation
            .apply_settings(&app, &new)
            .map_err(|e| format!("Не удалось применить сочетание клавиш: {e:#}"))?;
    }
    settings::save(&app, &new).map_err(|e| e.to_string())?;
    if old.engine != new.engine {
        preload_active_engine(app.clone());
        if new.engine == "parakeet" {
            transcriber_parakeet::spawn_mlx_install(app.clone());
        }
    }
    if old.summary_model != new.summary_model {
        summarizer_qwen3::stop_server();
    }
    if new.summarizer_enabled && (!old.summarizer_enabled || old.summary_model != new.summary_model)
    {
        summarizer_qwen3::preload_server(app.clone());
    }
    Ok(())
}

#[derive(Clone, Copy)]
pub(crate) enum SavedFileKind {
    Transcript,
    Summary,
}

pub(crate) fn validate_saved_file_path(
    app: &AppHandle,
    raw_path: &str,
    kind: SavedFileKind,
) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw_path);
    if !saved_file_kind_matches(&path, kind) {
        return Err("Неподдерживаемый тип файла".to_string());
    }
    let canonical_file = path
        .canonicalize()
        .map_err(|e| format!("Файл недоступен: {e}"))?;
    if !canonical_file.is_file() {
        return Err("Путь должен указывать на файл".to_string());
    }
    let allowed_dirs = allowed_saved_file_dirs(app);
    if allowed_dirs
        .iter()
        .all(|dir| !path_is_inside_dir(&canonical_file, dir))
    {
        return Err("Файл должен находиться в папке сохранения".to_string());
    }
    Ok(canonical_file)
}

fn allowed_saved_file_dirs(app: &AppHandle) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for dir in [settings::load(app).save_dir, paths::default_save_dir()] {
        if let Ok(canonical) = dir.canonicalize() {
            if !dirs.contains(&canonical) {
                dirs.push(canonical);
            }
        }
    }
    dirs
}

pub(crate) fn saved_file_kind_matches(path: &Path, kind: SavedFileKind) -> bool {
    match kind {
        SavedFileKind::Transcript => {
            path.extension()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case("txt"))
                == Some(true)
        }
        SavedFileKind::Summary => {
            path.file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.ends_with(".summary.md"))
                == Some(true)
        }
    }
}

fn path_is_inside_dir(path: &Path, dir: &Path) -> bool {
    path.starts_with(dir)
}

pub(crate) fn preload_active_engine(handle: AppHandle) {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = PRELOAD_LOCK.lock();
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
    let dictation = dictation::DictationManager::new();
    let dictation_for_shortcut = dictation.clone();
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, _shortcut, event| {
                    dictation_for_shortcut.handle_shortcut_event(app, event.state());
                })
                .build(),
        )
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(move |app| {
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
            let model_cancel = CancelRegistry::new();
            let summary_cancel_for_close = summary_cancel.clone();
            let model_cancel_for_close = model_cancel.clone();
            app.manage(AppState {
                queue,
                summary_cancel,
                model_cancel,
            });
            app.manage(dictation.clone());

            if let Some(win) = app.get_webview_window("main") {
                win.on_window_event(move |event| {
                    if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                        binaries::stop_yt_dlp_startup_cache();
                        transcriber_qwen::stop_server();
                        summarizer_qwen3::stop_server();
                        // Kill any running mlx-lm summarization subprocess so we
                        // don't leave a zombie Python process after window close.
                        summary_cancel_for_close.cancel_all();
                        model_cancel_for_close.cancel_all();
                    }
                });
            }

            preload_active_engine(handle.clone());
            if s.engine == "parakeet" {
                transcriber_parakeet::spawn_mlx_install(handle.clone());
            }
            if s.summarizer_enabled {
                summarizer_qwen3::preload_server(handle.clone());
            }
            binaries::warm_yt_dlp_startup_cache(handle.clone());
            dictation.start_worker(handle.clone());
            if let Err(e) = dictation.apply_settings(&handle, &s) {
                tracing::error!("dictation shortcut registration failed: {e:#}");
            }

            tracing::info!("Parrot started (engine: {})", s.engine);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::jobs::enqueue_file,
            commands::jobs::enqueue_youtube,
            get_settings,
            get_dictation_status,
            check_accessibility_permission,
            request_accessibility_permission,
            set_settings,
            commands::models::is_model_ready,
            commands::models::get_engine_statuses,
            commands::models::download_model,
            commands::models::download_model_for_engine,
            commands::models::delete_model,
            commands::models::delete_model_for_engine,
            commands::jobs::cancel_job,
            commands::system::open_in_finder,
            commands::system::open_logs,
            commands::system::log_client_error,
            commands::summary::get_summarizer_status,
            commands::summary::setup_summarizer_env,
            commands::summary::download_summarizer_model,
            commands::summary::delete_summarizer_model,
            commands::summary::summarize,
            commands::summary::cancel_summary,
            commands::history::get_history,
            commands::history::delete_history_entry,
            commands::history::clear_history,
            commands::history::load_history_entry,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_file_kind_should_accept_only_expected_outputs() {
        assert!(saved_file_kind_matches(
            Path::new("/tmp/audio.txt"),
            SavedFileKind::Transcript
        ));
        assert!(saved_file_kind_matches(
            Path::new("/tmp/audio.summary.md"),
            SavedFileKind::Summary
        ));
        assert!(!saved_file_kind_matches(
            Path::new("/tmp/audio.md"),
            SavedFileKind::Summary
        ));
        assert!(!saved_file_kind_matches(
            Path::new("/tmp/audio.summary.md"),
            SavedFileKind::Transcript
        ));
    }

    #[test]
    fn path_is_inside_dir_should_reject_siblings() {
        assert!(path_is_inside_dir(
            Path::new("/Users/alex/Documents/Transcripts/a.txt"),
            Path::new("/Users/alex/Documents/Transcripts")
        ));
        assert!(!path_is_inside_dir(
            Path::new("/Users/alex/Documents/Transcripts-old/a.txt"),
            Path::new("/Users/alex/Documents/Transcripts")
        ));
    }

    #[test]
    fn cancel_registry_guard_should_release_model_task_on_drop() {
        let registry = CancelRegistry::new();
        let task_id = "model:summary".to_string();
        assert!(registry.try_create(&task_id).is_some());

        {
            let _guard = CancelRegistryGuard::new(registry.clone(), task_id.clone());
            assert!(registry.get(&task_id).is_some());
        }

        assert!(registry.try_create(&task_id).is_some());
        registry.remove(&task_id);
    }
}
