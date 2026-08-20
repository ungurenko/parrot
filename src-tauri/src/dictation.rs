use anyhow::{anyhow, Result};
use core_foundation::base::{Boolean, CFTypeRef, TCFType};
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, KeyCode};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use parking_lot::Mutex;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::{paths, queue, settings, source};

const MAX_RECORD_SECONDS: u32 = 10 * 60;
const MIN_RECORD_SAMPLES: usize = 1_600;
const SILENCE_PEAK_THRESHOLD: f32 = 0.005;
const STREAM_FIRST_CALLBACK_TIMEOUT: Duration = Duration::from_millis(500);

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
    Start { insert_target: Option<InsertTarget> },
    Stop,
}

struct ActiveRecording {
    recorder: Recorder,
    insert_target: Option<InsertTarget>,
}

struct Recorder {
    stream: Stream,
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    started_at: Instant,
}

#[derive(Debug, Clone, Copy)]
struct RecordingStats {
    count: usize,
    peak: f32,
    sample_rate: u32,
    duration_secs: f32,
}

struct AxElement {
    raw: AXUIElementRef,
}

unsafe impl Send for AxElement {}

impl AxElement {
    fn new(raw: AXUIElementRef) -> Result<Self> {
        if raw.is_null() {
            anyhow::bail!("Не удалось найти активное поле ввода.");
        }
        Ok(Self { raw })
    }

    fn as_ptr(&self) -> AXUIElementRef {
        self.raw
    }
}

impl Drop for AxElement {
    fn drop(&mut self) {
        unsafe { CFRelease(self.raw as CFTypeRef) };
    }
}

struct InsertTarget {
    focused: AxElement,
}

unsafe impl Send for InsertTarget {}

impl InsertTarget {
    fn new(focused: AxElement) -> Self {
        Self { focused }
    }

    fn insert_text(&self, text: &str) -> Result<()> {
        set_selected_text(self.focused.as_ptr(), text)
    }

    fn focus(&self) -> Result<()> {
        let focused_attr = CFString::from_static_string("AXFocused");
        let value = core_foundation::boolean::CFBoolean::true_value();
        let set_error = unsafe {
            AXUIElementSetAttributeValue(
                self.focused.as_ptr(),
                focused_attr.as_concrete_TypeRef(),
                value.as_CFTypeRef(),
            )
        };

        if set_error != K_AX_ERROR_SUCCESS {
            anyhow::bail!("Не удалось вернуть курсор в исходное поле (AX error {set_error}).");
        }

        thread::sleep(Duration::from_millis(80));
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InsertAttempt {
    CapturedAccessibility,
    CapturedPaste,
    CurrentAccessibility,
    CurrentPaste,
}

fn insertion_attempts(has_captured_target: bool) -> &'static [InsertAttempt] {
    if has_captured_target {
        &[
            InsertAttempt::CapturedAccessibility,
            InsertAttempt::CapturedPaste,
        ]
    } else {
        &[
            InsertAttempt::CurrentAccessibility,
            InsertAttempt::CurrentPaste,
        ]
    }
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

        let insert_target = match capture_insert_target() {
            Ok(target) => Some(target),
            Err(e) => {
                tracing::warn!("dictation insert target capture failed: {e:#}");
                None
            }
        };

        match tx {
            Some(tx) => {
                if tx.send(RecorderCommand::Start { insert_target }).is_err() {
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
    let mut recorder: Option<ActiveRecording> = None;

    while let Ok(command) = rx.recv() {
        match command {
            RecorderCommand::Start { insert_target } => {
                if recorder.is_some() {
                    continue;
                }
                match Recorder::start() {
                    Ok(active) => {
                        recorder = Some(ActiveRecording {
                            recorder: active,
                            insert_target,
                        });
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
                let insert_target = active.insert_target;
                let result = write_recording_to_tmp(&app, active.recorder);
                match result {
                    Ok((raw_wav, normalized_wav, stats)) => {
                        tracing::info!(
                            "dictation recorded: {} samples ({:.2}s) @ {} Hz, peak amplitude {:.4}",
                            stats.count,
                            stats.duration_secs,
                            stats.sample_rate,
                            stats.peak
                        );

                        if stats.peak < SILENCE_PEAK_THRESHOLD {
                            cleanup_file(&raw_wav);
                            cleanup_file(&normalized_wav);
                            manager.set_error(
                                &app,
                                "Микрофон записал только тишину. Проверьте устройство ввода \
в Системные настройки → Звук → Вход и попробуйте сказать ближе к микрофону."
                                    .to_string(),
                            );
                            continue;
                        }

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
                                            "Речь не распознана. Попробуйте говорить громче или ближе к микрофону.".to_string(),
                                        );
                                        return;
                                    }
                                    match copy_to_clipboard(text.clone()).await {
                                        Ok(()) => match insert_text(text.clone(), insert_target)
                                            .await
                                        {
                                            Ok(()) => process_manager.mark_done(&process_app, text),
                                            Err(e) => process_manager.set_error(
                                                &process_app,
                                                format!("Текст скопирован, но не вставлен: {e:#}"),
                                            ),
                                        },
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
        let device_name = device.name().unwrap_or_else(|_| "<unknown>".to_string());
        let supported = device.default_input_config()?;
        let sample_rate = supported.sample_rate().0;
        let channels = supported.channels().max(1) as usize;
        let sample_format = supported.sample_format();
        tracing::info!(
            "dictation input device: {device_name}, sample_rate {sample_rate} Hz, channels {channels}, format {sample_format:?}"
        );
        let max_samples = sample_rate as usize * MAX_RECORD_SECONDS as usize;
        let config = supported.config();
        let samples = Arc::new(Mutex::new(Vec::<f32>::new()));
        let err_fn = |e| tracing::error!("dictation input stream error: {e}");

        let stream = match sample_format {
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
        let deadline = Instant::now() + STREAM_FIRST_CALLBACK_TIMEOUT;
        loop {
            if !samples.lock().is_empty() {
                break;
            }
            if Instant::now() >= deadline {
                anyhow::bail!(
                    "Микрофон не начал поставлять аудио. Проверьте устройство ввода в Системные настройки → Звук → Вход."
                );
            }
            thread::sleep(Duration::from_millis(10));
        }
        let started_at = Instant::now();
        Ok(Self {
            stream,
            samples,
            sample_rate,
            started_at,
        })
    }

    fn write_raw_wav(self, path: &Path) -> Result<RecordingStats> {
        drop(self.stream);
        if self.started_at.elapsed() < Duration::from_millis(180) {
            anyhow::bail!("Запись слишком короткая.");
        }
        let samples = self.samples.lock().clone();
        if samples.len() < MIN_RECORD_SAMPLES {
            anyhow::bail!("Запись слишком короткая.");
        }

        let count = samples.len();
        let peak = peak_amplitude(&samples);
        let duration_secs = count as f32 / self.sample_rate as f32;

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
        Ok(RecordingStats {
            count,
            peak,
            sample_rate: self.sample_rate,
            duration_secs,
        })
    }
}

fn peak_amplitude(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0_f32, |acc, s| acc.max(s.abs()))
}

fn write_recording_to_tmp(
    app: &AppHandle,
    recorder: Recorder,
) -> Result<(PathBuf, PathBuf, RecordingStats)> {
    let id = format!("dictation-{}", uuid::Uuid::new_v4());
    let tmp = paths::tmp_dir(app)?;
    let raw_wav = tmp.join(format!("{id}.raw.wav"));
    let normalized_wav = tmp.join(format!("{id}.wav"));
    let stats = match recorder.write_raw_wav(&raw_wav) {
        Ok(stats) => stats,
        Err(e) => {
            cleanup_file(&raw_wav);
            return Err(e);
        }
    };
    Ok((raw_wav, normalized_wav, stats))
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
        queue::transcribe_wav_for_engine(
            &app_for_task,
            &wav_for_task,
            &engine,
            &language,
            None,
            progress,
        )
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

async fn insert_text(text: String, target: Option<InsertTarget>) -> Result<()> {
    tokio::task::spawn_blocking(move || -> Result<()> {
        let mut last_error = None;
        for attempt in insertion_attempts(target.is_some()) {
            let result = match attempt {
                InsertAttempt::CapturedAccessibility => target
                    .as_ref()
                    .ok_or_else(|| anyhow!("Исходное поле для диктовки не найдено."))
                    .and_then(|target| target.insert_text(&text)),
                InsertAttempt::CapturedPaste => target
                    .as_ref()
                    .ok_or_else(|| anyhow!("Исходное поле для диктовки не найдено."))
                    .and_then(|target| {
                        target.focus()?;
                        send_command_v()
                    }),
                InsertAttempt::CurrentAccessibility => insert_text_with_accessibility(&text),
                InsertAttempt::CurrentPaste => send_command_v(),
            };

            match result {
                Ok(()) => {
                    tracing::info!("dictation text inserted through {attempt:?}");
                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!("dictation insertion attempt {attempt:?} failed: {e:#}");
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("Не удалось вставить текст диктовки.")))
    })
    .await?
}

fn send_command_v() -> Result<()> {
    ensure_accessibility_permission()?;

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| anyhow!("Не удалось создать системное событие клавиатуры."))?;
    let flags = CGEventFlags::CGEventFlagCommand;

    post_key_event(&source, KeyCode::COMMAND, true, flags)?;
    thread::sleep(Duration::from_millis(20));
    post_key_event(&source, KeyCode::ANSI_V, true, flags)?;
    thread::sleep(Duration::from_millis(20));
    post_key_event(&source, KeyCode::ANSI_V, false, flags)?;
    thread::sleep(Duration::from_millis(20));
    post_key_event(&source, KeyCode::COMMAND, false, CGEventFlags::empty())?;
    tracing::info!("dictation paste shortcut posted");
    Ok(())
}

fn post_key_event(
    source: &CGEventSource,
    key_code: u16,
    key_down: bool,
    flags: CGEventFlags,
) -> Result<()> {
    let event = CGEvent::new_keyboard_event(source.clone(), key_code, key_down)
        .map_err(|_| anyhow!("Не удалось создать системное событие клавиатуры."))?;
    event.set_flags(flags);
    event.post(CGEventTapLocation::HID);
    Ok(())
}

fn insert_text_with_accessibility(text: &str) -> Result<()> {
    ensure_accessibility_permission()?;

    let focused = copy_focused_ui_element()?;
    set_selected_text(focused.as_ptr(), text)
}

fn capture_insert_target() -> Result<InsertTarget> {
    ensure_accessibility_permission()?;

    copy_focused_ui_element().map(InsertTarget::new)
}

fn copy_focused_ui_element() -> Result<AxElement> {
    let system = unsafe { AXUIElementCreateSystemWide() };
    if system.is_null() {
        anyhow::bail!("Не удалось получить системный Accessibility-элемент.");
    }

    let focused_attr = CFString::from_static_string("AXFocusedUIElement");
    let mut focused: CFTypeRef = ptr::null();
    let copy_error = unsafe {
        AXUIElementCopyAttributeValue(
            system,
            focused_attr.as_concrete_TypeRef(),
            &mut focused as *mut CFTypeRef,
        )
    };
    unsafe { CFRelease(system as CFTypeRef) };

    if copy_error != K_AX_ERROR_SUCCESS || focused.is_null() {
        anyhow::bail!("Не удалось найти активное поле ввода (AX error {copy_error}).");
    }

    AxElement::new(focused as AXUIElementRef)
}

fn set_selected_text(element: AXUIElementRef, text: &str) -> Result<()> {
    let replacement = CFString::new(text);
    let selected_text_attr = CFString::from_static_string("AXSelectedText");
    let set_error = unsafe {
        AXUIElementSetAttributeValue(
            element,
            selected_text_attr.as_concrete_TypeRef(),
            replacement.as_CFTypeRef(),
        )
    };

    if set_error != K_AX_ERROR_SUCCESS {
        anyhow::bail!("Активное поле не приняло прямую вставку (AX error {set_error}).");
    }

    Ok(())
}

pub(crate) fn accessibility_permission_granted() -> bool {
    ax_is_process_trusted(false)
}

pub(crate) fn request_accessibility_permission() -> bool {
    ax_is_process_trusted(true)
}

fn ensure_accessibility_permission() -> Result<()> {
    if request_accessibility_permission() {
        Ok(())
    } else {
        anyhow::bail!(
            "macOS не разрешает Parrot вставлять текст автоматически. Разрешите Parrot в Системные настройки → Конфиденциальность и безопасность → Универсальный доступ."
        )
    }
}

#[cfg(target_os = "macos")]
fn ax_is_process_trusted(prompt: bool) -> bool {
    let key = unsafe { CFString::wrap_under_get_rule(K_AX_TRUSTED_CHECK_OPTION_PROMPT) };
    let value = if prompt {
        core_foundation::boolean::CFBoolean::true_value()
    } else {
        core_foundation::boolean::CFBoolean::false_value()
    };
    let options = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), value.as_CFType())]);
    unsafe { AXIsProcessTrustedWithOptions(options.as_CFTypeRef()) != 0 }
}

#[cfg(not(target_os = "macos"))]
fn ax_is_process_trusted(_prompt: bool) -> bool {
    true
}

#[cfg(target_os = "macos")]
type AXUIElementRef = *const std::ffi::c_void;

#[cfg(target_os = "macos")]
const K_AX_ERROR_SUCCESS: i32 = 0;

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    #[link_name = "kAXTrustedCheckOptionPrompt"]
    static K_AX_TRUSTED_CHECK_OPTION_PROMPT: CFStringRef;

    fn AXIsProcessTrustedWithOptions(options: CFTypeRef) -> Boolean;
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> i32;
    fn CFRelease(cf: CFTypeRef);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_shortcut_should_accept_option_q() {
        let shortcut = normalize_shortcut("Option+q").unwrap();

        assert_eq!(shortcut, "Alt+Q");
    }

    #[test]
    fn normalize_shortcut_should_accept_command_shift_space() {
        let shortcut = normalize_shortcut("cmd+shift+space").unwrap();

        assert_eq!(shortcut, "Command+Shift+Space");
    }

    #[test]
    fn normalize_shortcut_should_reject_plain_key_without_modifier() {
        let err = normalize_shortcut("Q").unwrap_err();

        assert_eq!(
            err.to_string(),
            "Сочетание должно содержать хотя бы одну служебную клавишу и обычную клавишу."
        );
    }

    #[test]
    fn normalize_shortcut_part_should_keep_function_keys_uppercase() {
        let key = normalize_shortcut_part("f12").unwrap();

        assert_eq!(key, "F12");
    }

    #[test]
    fn peak_amplitude_should_return_max_absolute_value() {
        let samples = vec![0.01_f32, -0.4_f32, 0.0_f32, 0.25_f32, -0.7_f32];
        assert!((peak_amplitude(&samples) - 0.7).abs() < 1e-6);
    }

    #[test]
    fn peak_amplitude_below_threshold_marks_silence() {
        let samples: Vec<f32> = (0..1000).map(|_| 0.001_f32).collect();
        assert!(peak_amplitude(&samples) < SILENCE_PEAK_THRESHOLD);
    }

    #[test]
    fn peak_amplitude_handles_empty_slice() {
        let samples: Vec<f32> = Vec::new();
        assert_eq!(peak_amplitude(&samples), 0.0);
    }

    #[test]
    fn insertion_attempts_with_captured_target_should_not_use_current_focus() {
        assert_eq!(
            insertion_attempts(true),
            &[
                InsertAttempt::CapturedAccessibility,
                InsertAttempt::CapturedPaste,
            ]
        );
    }

    #[test]
    fn insertion_attempts_without_captured_target_should_use_current_focus() {
        assert_eq!(
            insertion_attempts(false),
            &[
                InsertAttempt::CurrentAccessibility,
                InsertAttempt::CurrentPaste,
            ]
        );
    }
}
