use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use parking_lot::Mutex;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::cancellation::CancelToken;
use crate::{paths, queue, settings, source, transcriber, transcriber_parakeet, transcriber_qwen};

const MAX_RECORD_SECONDS: u32 = 10 * 60;
const MIN_RECORD_SAMPLES: usize = 1_600;

#[derive(Clone)]
pub struct DictationManager {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    phase: DictationPhase,
    command_tx: Option<mpsc::Sender<RecorderCommand>>,
    worker_started: bool,
    registered_shortcut: Option<String>,
    last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DictationPhase {
    Idle,
    Recording,
    Processing,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictationStatus {
    pub enabled: bool,
    pub hold_key: String,
    pub phase: DictationPhase,
    pub listener_started: bool,
    pub registered_shortcut: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictationDoneEvent {
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictationErrorEvent {
    pub message: String,
}

enum RecorderCommand {
    Start,
    Stop,
}

struct Recorder {
    stream: Stream,
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    started_at: Instant,
}

impl DictationManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                phase: DictationPhase::Idle,
                command_tx: None,
                worker_started: false,
                registered_shortcut: None,
                last_error: None,
            })),
        }
    }

    pub fn status(&self, app: &AppHandle) -> DictationStatus {
        let settings = settings::load(app);
        let inner = self.inner.lock();
        DictationStatus {
            enabled: settings.dictation_enabled,
            hold_key: settings.dictation_hold_key,
            phase: inner.phase,
            listener_started: inner.worker_started,
            registered_shortcut: inner.registered_shortcut.clone(),
            last_error: inner.last_error.clone(),
        }
    }

    pub fn start_worker(&self, app: AppHandle) {
        let (tx, rx) = mpsc::channel();
        {
            let mut inner = self.inner.lock();
            if inner.worker_started {
                return;
            }
            inner.worker_started = true;
            inner.command_tx = Some(tx.clone());
        }

        let worker_manager = self.clone();
        let worker_app = app.clone();
        std::thread::spawn(move || recording_worker(worker_app, worker_manager, rx));
    }

    pub fn apply_settings(&self, app: &AppHandle, settings: &settings::Settings) -> Result<()> {
        let shortcut = normalize_shortcut(&settings.dictation_hold_key)?;
        let old_shortcut = self.inner.lock().registered_shortcut.clone();

        if !settings.dictation_enabled {
            if let Some(old) = old_shortcut {
                app.global_shortcut().unregister(old.as_str())?;
            }
            self.inner.lock().registered_shortcut = None;
            return Ok(());
        }

        if old_shortcut.as_deref() == Some(shortcut.as_str()) {
            return Ok(());
        }

        app.global_shortcut().register(shortcut.as_str())?;
        if let Some(old) = old_shortcut {
            app.global_shortcut().unregister(old.as_str())?;
        }
        self.inner.lock().registered_shortcut = Some(shortcut);
        Ok(())
    }

    pub fn handle_shortcut_event(&self, app: &AppHandle, state: ShortcutState) {
        match state {
            ShortcutState::Pressed => self.handle_shortcut_pressed(app),
            ShortcutState::Released => self.handle_shortcut_released(app),
        }
    }

    fn handle_shortcut_pressed(&self, app: &AppHandle) {
        let settings = settings::load(app);
        if !settings.dictation_enabled {
            return;
        }

        let tx = {
            let mut inner = self.inner.lock();
            if !matches!(inner.phase, DictationPhase::Idle | DictationPhase::Error) {
                return;
            }
            inner.phase = DictationPhase::Recording;
            inner.last_error = None;
            inner.command_tx.clone()
        };

        match tx {
            Some(tx) => {
                if tx.send(RecorderCommand::Start).is_err() {
                    self.set_error(
                        app,
                        "Диктовка не запущена: внутренний канал закрыт.".to_string(),
                    );
                }
            }
            None => self.set_error(app, "Диктовка ещё не готова.".to_string()),
        }
    }

    fn handle_shortcut_released(&self, app: &AppHandle) {
        let tx = {
            let mut inner = self.inner.lock();
            if inner.phase != DictationPhase::Recording {
                return;
            }
            inner.phase = DictationPhase::Processing;
            inner.command_tx.clone()
        };

        match tx {
            Some(tx) => {
                if tx.send(RecorderCommand::Stop).is_err() {
                    self.set_error(
                        app,
                        "Диктовка не остановлена: внутренний канал закрыт.".to_string(),
                    );
                }
            }
            None => self.set_error(app, "Диктовка ещё не готова.".to_string()),
        }
    }

    fn mark_recording(&self, app: &AppHandle) {
        {
            let mut inner = self.inner.lock();
            inner.phase = DictationPhase::Recording;
            inner.last_error = None;
        }
        let _ = app.emit("dictation:started", ());
    }

    fn mark_processing(&self, app: &AppHandle) {
        {
            let mut inner = self.inner.lock();
            inner.phase = DictationPhase::Processing;
        }
        let _ = app.emit("dictation:processing", ());
    }

    fn mark_done(&self, app: &AppHandle, text: String) {
        {
            let mut inner = self.inner.lock();
            inner.phase = DictationPhase::Idle;
            inner.last_error = None;
        }
        let _ = app.emit("dictation:done", DictationDoneEvent { text });
    }

    fn set_error(&self, app: &AppHandle, message: String) {
        {
            let mut inner = self.inner.lock();
            inner.phase = DictationPhase::Error;
            inner.last_error = Some(message.clone());
        }
        let _ = app.emit("dictation:error", DictationErrorEvent { message });
    }
}

fn recording_worker(
    app: AppHandle,
    manager: DictationManager,
    rx: mpsc::Receiver<RecorderCommand>,
) {
    let mut recorder: Option<Recorder> = None;

    while let Ok(command) = rx.recv() {
        match command {
            RecorderCommand::Start => {
                if recorder.is_some() {
                    continue;
                }
                match Recorder::start() {
                    Ok(active) => {
                        recorder = Some(active);
                        manager.mark_recording(&app);
                    }
                    Err(e) => manager.set_error(&app, format!("{e:#}")),
                }
            }
            RecorderCommand::Stop => {
                let Some(active) = recorder.take() else {
                    manager.set_error(&app, "Запись не была запущена.".to_string());
                    continue;
                };

                manager.mark_processing(&app);
                let result = write_recording_to_tmp(&app, active);
                match result {
                    Ok((raw_wav, normalized_wav)) => {
                        let process_app = app.clone();
                        let process_manager = manager.clone();
                        tauri::async_runtime::spawn(async move {
                            let result =
                                process_wav_files(&process_app, &raw_wav, &normalized_wav).await;
                            cleanup_file(&raw_wav);
                            cleanup_file(&normalized_wav);

                            match result {
                                Ok(text) => {
                                    if text.trim().is_empty() {
                                        process_manager.set_error(
                                            &process_app,
                                            "Речь не распознана, буфер не изменён.".to_string(),
                                        );
                                        return;
                                    }
                                    match copy_to_clipboard(text.clone()).await {
                                        Ok(()) => process_manager.mark_done(&process_app, text),
                                        Err(e) => process_manager.set_error(
                                            &process_app,
                                            format!("Не удалось скопировать текст: {e:#}"),
                                        ),
                                    }
                                }
                                Err(e) => process_manager.set_error(&process_app, format!("{e:#}")),
                            }
                        });
                    }
                    Err(e) => manager.set_error(&app, format!("{e:#}")),
                }
            }
        }
    }
}

impl Recorder {
    fn start() -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow!("Микрофон не найден."))?;
        let supported = device.default_input_config()?;
        let sample_rate = supported.sample_rate().0;
        let channels = supported.channels().max(1) as usize;
        let max_samples = sample_rate as usize * MAX_RECORD_SECONDS as usize;
        let config = supported.config();
        let samples = Arc::new(Mutex::new(Vec::<f32>::new()));
        let err_fn = |e| tracing::error!("dictation input stream error: {e}");

        let stream = match supported.sample_format() {
            SampleFormat::F32 => {
                let samples = samples.clone();
                device.build_input_stream(
                    &config,
                    move |data: &[f32], _| push_f32_samples(data, channels, max_samples, &samples),
                    err_fn,
                    None,
                )?
            }
            SampleFormat::I16 => {
                let samples = samples.clone();
                device.build_input_stream(
                    &config,
                    move |data: &[i16], _| push_i16_samples(data, channels, max_samples, &samples),
                    err_fn,
                    None,
                )?
            }
            SampleFormat::U16 => {
                let samples = samples.clone();
                device.build_input_stream(
                    &config,
                    move |data: &[u16], _| push_u16_samples(data, channels, max_samples, &samples),
                    err_fn,
                    None,
                )?
            }
            other => anyhow::bail!("Неподдерживаемый формат микрофона: {other:?}"),
        };

        stream.play()?;
        Ok(Self {
            stream,
            samples,
            sample_rate,
            started_at: Instant::now(),
        })
    }

    fn write_raw_wav(self, path: &Path) -> Result<()> {
        drop(self.stream);
        if self.started_at.elapsed() < Duration::from_millis(180) {
            anyhow::bail!("Запись слишком короткая.");
        }
        let samples = self.samples.lock().clone();
        if samples.len() < MIN_RECORD_SAMPLES {
            anyhow::bail!("Запись слишком короткая.");
        }

        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: self.sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec)?;
        for sample in samples {
            let value = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            writer.write_sample(value)?;
        }
        writer.finalize()?;
        Ok(())
    }
}

fn write_recording_to_tmp(app: &AppHandle, recorder: Recorder) -> Result<(PathBuf, PathBuf)> {
    let id = format!("dictation-{}", uuid::Uuid::new_v4());
    let tmp = paths::tmp_dir(app)?;
    let raw_wav = tmp.join(format!("{id}.raw.wav"));
    let normalized_wav = tmp.join(format!("{id}.wav"));
    if let Err(e) = recorder.write_raw_wav(&raw_wav) {
        cleanup_file(&raw_wav);
        return Err(e);
    }
    Ok((raw_wav, normalized_wav))
}

async fn process_wav_files(
    app: &AppHandle,
    raw_wav: &Path,
    normalized_wav: &Path,
) -> Result<String> {
    source::local::extract_wav(app, raw_wav, normalized_wav, None).await?;
    transcribe_dictation_wav(app, normalized_wav).await
}

async fn transcribe_dictation_wav(app: &AppHandle, wav_path: &Path) -> Result<String> {
    let settings = settings::load(app);
    queue::ensure_model_ready(app, &settings.engine)?;

    let app_for_task = app.clone();
    let wav_for_task = wav_path.to_path_buf();
    let engine = settings.engine;
    let language = settings.language;
    tokio::task::spawn_blocking(move || -> Result<String> {
        let progress = |_p: u32| {};
        match engine.as_str() {
            "parakeet" => {
                let dir = paths::parakeet_dir(&app_for_task)?;
                transcriber_parakeet::transcribe_wav(&dir, &wav_for_task, progress)
            }
            "whisper" => {
                let model = paths::model_path(&app_for_task)?;
                transcriber::transcribe_wav(&model, &wav_for_task, &language, progress)
            }
            engine if transcriber_qwen::is_qwen_engine(engine) => transcriber_qwen::transcribe_wav(
                &app_for_task,
                &wav_for_task,
                engine,
                &language,
                None::<Arc<CancelToken>>,
                progress,
            ),
            other => anyhow::bail!("Неизвестный движок транскрибации: {other}"),
        }
    })
    .await?
}

async fn copy_to_clipboard(text: String) -> Result<()> {
    tokio::task::spawn_blocking(move || -> Result<()> {
        let mut clipboard = arboard::Clipboard::new()?;
        clipboard.set_text(text)?;
        Ok(())
    })
    .await?
}

fn push_f32_samples(
    data: &[f32],
    channels: usize,
    max_samples: usize,
    samples: &Arc<Mutex<Vec<f32>>>,
) {
    push_mono_samples(
        data.chunks(channels).map(average_f32_frame),
        max_samples,
        samples,
    );
}

fn push_i16_samples(
    data: &[i16],
    channels: usize,
    max_samples: usize,
    samples: &Arc<Mutex<Vec<f32>>>,
) {
    push_mono_samples(
        data.chunks(channels).map(|frame| {
            frame.iter().map(|s| *s as f32 / 32_768.0).sum::<f32>() / frame.len().max(1) as f32
        }),
        max_samples,
        samples,
    );
}

fn push_u16_samples(
    data: &[u16],
    channels: usize,
    max_samples: usize,
    samples: &Arc<Mutex<Vec<f32>>>,
) {
    push_mono_samples(
        data.chunks(channels).map(|frame| {
            frame
                .iter()
                .map(|s| (*s as f32 - 32_768.0) / 32_768.0)
                .sum::<f32>()
                / frame.len().max(1) as f32
        }),
        max_samples,
        samples,
    );
}

fn push_mono_samples<I>(incoming: I, max_samples: usize, samples: &Arc<Mutex<Vec<f32>>>)
where
    I: IntoIterator<Item = f32>,
{
    let mut guard = samples.lock();
    let remaining = max_samples.saturating_sub(guard.len());
    if remaining == 0 {
        return;
    }
    guard.extend(incoming.into_iter().take(remaining));
}

fn average_f32_frame(frame: &[f32]) -> f32 {
    frame.iter().sum::<f32>() / frame.len().max(1) as f32
}

fn cleanup_file(path: &PathBuf) {
    if let Err(e) = std::fs::remove_file(path) {
        if path.exists() {
            tracing::warn!(
                "failed to remove dictation temp file {}: {e:#}",
                path.display()
            );
        }
    }
}

fn normalize_shortcut(shortcut: &str) -> Result<String> {
    let parts: Vec<String> = shortcut
        .split('+')
        .map(|part| normalize_shortcut_part(part.trim()))
        .collect::<Result<Vec<_>>>()?;
    if parts.len() < 2 {
        anyhow::bail!(
            "Сочетание должно содержать хотя бы одну служебную клавишу и обычную клавишу."
        );
    }
    let has_modifier = parts
        .iter()
        .take(parts.len().saturating_sub(1))
        .any(|part| {
            matches!(
                part.as_str(),
                "Alt" | "Control" | "Command" | "Shift" | "Super"
            )
        });
    if !has_modifier {
        anyhow::bail!("Добавьте Command, Option, Control или Shift.");
    }
    Ok(parts.join("+"))
}

fn normalize_shortcut_part(part: &str) -> Result<String> {
    if part.is_empty() {
        anyhow::bail!("Пустая часть сочетания клавиш.");
    }
    let normalized = match part.to_ascii_lowercase().as_str() {
        "option" | "alt" | "⌥" => "Alt".to_string(),
        "control" | "ctrl" | "⌃" => "Control".to_string(),
        "command" | "cmd" | "meta" | "⌘" => "Command".to_string(),
        "shift" | "⇧" => "Shift".to_string(),
        "space" | "пробел" | " " => "Space".to_string(),
        "escape" | "esc" => "Escape".to_string(),
        "enter" | "return" => "Enter".to_string(),
        "backspace" => "Backspace".to_string(),
        "tab" => "Tab".to_string(),
        one if one.len() == 1 => one.to_ascii_uppercase(),
        other => {
            if let Some(rest) = other.strip_prefix('f') {
                if rest
                    .parse::<u8>()
                    .map(|n| (1..=24).contains(&n))
                    .unwrap_or(false)
                {
                    return Ok(format!("F{rest}"));
                }
            }
            part.to_string()
        }
    };
    Ok(normalized)
}
