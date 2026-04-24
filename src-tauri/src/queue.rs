use anyhow::Result;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;

use crate::cancellation::{CancelRegistry, CancelToken};
use crate::history::{self, HistoryEntry};
use crate::{paths, source, transcriber, transcriber_parakeet, transcriber_qwen, writer};

const CANCELLED_MESSAGE: &str = "Отменено пользователем";

const PREP_WORKERS: usize = 2;

#[derive(Debug, Clone)]
pub enum SourceKind {
    LocalFile(PathBuf),
    YouTube(String),
}

#[derive(Debug, Clone)]
pub struct Job {
    pub id: String,
    pub source: SourceKind,
    pub display_name: String,
    pub engine: String,
    pub language: String,
    pub save_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobQueuedEvent {
    pub id: String,
    pub source_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobProgressEvent {
    pub id: String,
    pub stage: String,
    pub percent: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobDoneEvent {
    pub id: String,
    pub text: String,
    pub output_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobErrorEvent {
    pub id: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobCanceledEvent {
    pub id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobTitleEvent {
    pub id: String,
    pub source_name: String,
}

#[derive(Debug, Clone)]
struct QueuedJob {
    sequence: u64,
    job: Job,
}

#[derive(Debug)]
struct PreparedJob {
    sequence: u64,
    job: Job,
    wav_path: PathBuf,
    stem: String,
    prepared_in: Duration,
}

#[derive(Debug)]
enum PreparedOutcome {
    Ready(PreparedJob),
    Failed {
        sequence: u64,
        job: Job,
        message: String,
    },
}

impl PreparedOutcome {
    fn sequence(&self) -> u64 {
        match self {
            Self::Ready(prepared) => prepared.sequence,
            Self::Failed { sequence, .. } => *sequence,
        }
    }
}

pub struct JobQueue {
    sender: Mutex<mpsc::UnboundedSender<QueuedJob>>,
    next_sequence: AtomicU64,
    app: AppHandle,
    cancel: CancelRegistry,
}

impl JobQueue {
    pub fn cancel_registry(&self) -> CancelRegistry {
        self.cancel.clone()
    }

    pub fn app_handle(&self) -> AppHandle {
        self.app.clone()
    }
}

impl JobQueue {
    pub fn start(app: AppHandle) -> Arc<Self> {
        let (prep_tx, prep_rx) = mpsc::unbounded_channel::<QueuedJob>();
        let (ready_tx, ready_rx) = mpsc::unbounded_channel::<PreparedOutcome>();
        let cancel = CancelRegistry::new();
        let queue = Arc::new(Self {
            sender: Mutex::new(prep_tx),
            next_sequence: AtomicU64::new(0),
            app: app.clone(),
            cancel: cancel.clone(),
        });

        let prep_rx = Arc::new(Mutex::new(prep_rx));
        for worker_idx in 0..PREP_WORKERS {
            tauri::async_runtime::spawn(prep_worker(
                app.clone(),
                worker_idx,
                prep_rx.clone(),
                ready_tx.clone(),
                cancel.clone(),
            ));
        }
        drop(ready_tx);

        tauri::async_runtime::spawn(transcribe_worker(app, ready_rx, cancel));
        queue
    }

    pub async fn enqueue(&self, job: Job) -> Result<()> {
        let sequence = self.next_sequence.fetch_add(1, Ordering::Relaxed);
        self.cancel.create(&job.id);
        let _ = self.app.emit(
            "job:queued",
            JobQueuedEvent {
                id: job.id.clone(),
                source_name: job.display_name.clone(),
            },
        );
        let tx = self.sender.lock().await;
        tx.send(QueuedJob {
            sequence,
            job: job.clone(),
        })
        .map_err(|e| {
            self.cancel.remove(&job.id);
            anyhow::anyhow!(e.to_string())
        })?;
        Ok(())
    }
}

async fn prep_worker(
    app: AppHandle,
    worker_idx: usize,
    rx: Arc<Mutex<mpsc::UnboundedReceiver<QueuedJob>>>,
    ready_tx: mpsc::UnboundedSender<PreparedOutcome>,
    cancel: CancelRegistry,
) {
    loop {
        let queued = {
            let mut rx = rx.lock().await;
            rx.recv().await
        };
        let Some(queued) = queued else {
            break;
        };

        let token = cancel.get(&queued.job.id);
        if token.as_ref().map(|t| t.is_cancelled()).unwrap_or(false) {
            let outcome = PreparedOutcome::Failed {
                sequence: queued.sequence,
                job: queued.job,
                message: CANCELLED_MESSAGE.to_string(),
            };
            if ready_tx.send(outcome).is_err() {
                break;
            }
            continue;
        }

        tracing::info!("prep worker {worker_idx} started job {}", queued.job.id);
        let outcome = match prepare_job(&app, queued.clone(), token.clone()).await {
            Ok(prepared) => PreparedOutcome::Ready(prepared),
            Err(e) => {
                let message = if token.as_ref().map(|t| t.is_cancelled()).unwrap_or(false) {
                    CANCELLED_MESSAGE.to_string()
                } else {
                    format!("{e:#}")
                };
                PreparedOutcome::Failed {
                    sequence: queued.sequence,
                    job: queued.job,
                    message,
                }
            }
        };

        if ready_tx.send(outcome).is_err() {
            break;
        }
    }
}

async fn transcribe_worker(
    app: AppHandle,
    mut rx: mpsc::UnboundedReceiver<PreparedOutcome>,
    cancel: CancelRegistry,
) {
    let mut next_sequence = 0u64;
    let mut pending = BTreeMap::new();

    while let Some(outcome) = rx.recv().await {
        pending.insert(outcome.sequence(), outcome);

        while let Some(outcome) = pending.remove(&next_sequence) {
            handle_prepared_outcome(&app, outcome, &cancel).await;
            next_sequence += 1;
        }
    }
}

async fn handle_prepared_outcome(
    app: &AppHandle,
    outcome: PreparedOutcome,
    cancel: &CancelRegistry,
) {
    match outcome {
        PreparedOutcome::Ready(prepared) => {
            let job = prepared.job.clone();
            let wav_path = prepared.wav_path.clone();
            let token = cancel.get(&job.id);
            if token.as_ref().map(|t| t.is_cancelled()).unwrap_or(false) {
                emit_canceled(app, &job);
                cancel.remove(&job.id);
                cleanup_wav(&prepared.wav_path);
                return;
            }
            match transcribe_prepared(app, prepared, token.clone()).await {
                Ok((text, out)) => {
                    let output_path_str = out.to_string_lossy().to_string();
                    let entry = HistoryEntry {
                        id: job.id.clone(),
                        source_name: job.display_name.clone(),
                        engine: job.engine.clone(),
                        language: job.language.clone(),
                        created_at: history::now_iso8601(),
                        output_path: output_path_str.clone(),
                        summary_path: None,
                    };
                    if let Err(e) = history::append(app, entry) {
                        tracing::warn!("history append failed for job {}: {e:#}", job.id);
                    } else {
                        let _ = app.emit("history:updated", ());
                    }
                    let _ = app.emit(
                        "job:done",
                        JobDoneEvent {
                            id: job.id.clone(),
                            text,
                            output_path: output_path_str,
                        },
                    );
                    cancel.remove(&job.id);
                }
                Err(e) => {
                    let message = if token.as_ref().map(|t| t.is_cancelled()).unwrap_or(false) {
                        CANCELLED_MESSAGE.to_string()
                    } else {
                        format!("{e:#}")
                    };
                    if message == CANCELLED_MESSAGE {
                        emit_canceled(app, &job);
                    } else {
                        tracing::error!("job {} failed: {message}", job.id);
                        emit_error(app, &job, message);
                    }
                    cancel.remove(&job.id);
                    cleanup_wav(&wav_path);
                }
            }
        }
        PreparedOutcome::Failed { job, message, .. } => {
            if message == CANCELLED_MESSAGE {
                emit_canceled(app, &job);
            } else {
                tracing::error!("job {} failed: {message}", job.id);
                emit_error(app, &job, message);
            }
            cancel.remove(&job.id);
        }
    }
}

async fn prepare_job(
    app: &AppHandle,
    queued: QueuedJob,
    token: Option<Arc<CancelToken>>,
) -> Result<PreparedJob> {
    let started = Instant::now();

    let mut job = queued.job;
    ensure_model_ready(app, &job.engine)?;
    let tmp = paths::tmp_dir(app)?;
    let wav_path = tmp.join(format!("{}.wav", job.id));

    let stem = match &job.source {
        SourceKind::LocalFile(path) => {
            emit_progress(app, &job.id, "extracting", 0);
            let extracting_started = Instant::now();
            source::local::extract_wav(app, path, &wav_path, token.clone()).await?;
            emit_progress(app, &job.id, "extracting", 100);
            tracing::info!(
                "job {} extracted local audio in {:.2}s",
                job.id,
                seconds(extracting_started.elapsed())
            );
            path.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "transcript".to_string())
        }
        SourceKind::YouTube(url) => {
            emit_progress(app, &job.id, "preparing", 1);
            let app_for_cb = app.clone();
            let id_for_cb = job.id.clone();
            let downloading_started = Instant::now();
            let yt =
                source::youtube::download_wav(app, url, &tmp, &job.id, token.clone(), move |p| {
                    emit_progress(&app_for_cb, &id_for_cb, "downloading", p.max(1));
                })
                .await?;
            emit_progress(app, &job.id, "downloading", 100);
            tracing::info!(
                "job {} downloaded and normalized YouTube audio in {:.2}s",
                job.id,
                seconds(downloading_started.elapsed())
            );
            if yt.wav_path != wav_path {
                std::fs::rename(&yt.wav_path, &wav_path)?;
            }
            if !yt.title.trim().is_empty() && yt.title != job.display_name {
                job.display_name = yt.title.clone();
                let _ = app.emit(
                    "job:title",
                    JobTitleEvent {
                        id: job.id.clone(),
                        source_name: yt.title.clone(),
                    },
                );
            }
            yt.title
        }
    };

    let prepared_in = started.elapsed();
    tracing::info!("job {} prepared in {:.2}s", job.id, seconds(prepared_in));

    Ok(PreparedJob {
        sequence: queued.sequence,
        job,
        wav_path,
        stem,
        prepared_in,
    })
}

async fn transcribe_prepared(
    app: &AppHandle,
    prepared: PreparedJob,
    token: Option<Arc<CancelToken>>,
) -> Result<(String, PathBuf)> {
    let PreparedJob {
        job,
        wav_path,
        stem,
        prepared_in,
        ..
    } = prepared;
    let engine = job.engine.clone();
    let language = job.language.clone();
    ensure_model_ready(app, &engine)?;

    emit_progress(app, &job.id, "transcribing", 1);
    let app_for_cb = app.clone();
    let job_id = job.id.clone();
    let engine_for_task = engine.clone();
    let language_for_task = language.clone();
    let app_for_task = app.clone();
    let wav_for_task = wav_path.clone();
    let token_for_task = token.clone();
    let transcribing_started = Instant::now();

    // Qwen CLI runs as a subprocess without intermediate progress output.
    // Simulate progress (5 → 95%) based on estimated transcription time.
    let ticker_stop = Arc::new(AtomicBool::new(false));
    let ticker_handle = if transcriber_qwen::is_qwen_engine(&engine_for_task) {
        let duration_sec = wav_duration_seconds(&wav_for_task).unwrap_or(60.0);
        let rtf: f64 = if engine_for_task == transcriber_qwen::ENGINE_QWEN_1_7B {
            0.9
        } else {
            0.45
        };
        let expected_sec = (duration_sec * rtf).max(4.0);
        let ticker_app = app.clone();
        let ticker_job_id = job.id.clone();
        let ticker_flag = ticker_stop.clone();
        let started = Instant::now();
        Some(tauri::async_runtime::spawn(async move {
            while !ticker_flag.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_millis(700)).await;
                if ticker_flag.load(Ordering::Relaxed) {
                    break;
                }
                let elapsed = started.elapsed().as_secs_f64();
                let ratio = (elapsed / expected_sec).min(0.95);
                let pct = (5.0 + ratio * 90.0) as u32;
                let _ = ticker_app.emit(
                    "job:progress",
                    JobProgressEvent {
                        id: ticker_job_id.clone(),
                        stage: "transcribing".to_string(),
                        percent: pct.max(5),
                    },
                );
            }
        }))
    } else {
        None
    };
    let text = tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
        let progress = move |p: u32| {
            let _ = app_for_cb.emit(
                "job:progress",
                JobProgressEvent {
                    id: job_id.clone(),
                    stage: "transcribing".to_string(),
                    percent: p,
                },
            );
        };
        match engine_for_task.as_str() {
            "parakeet" => {
                let dir = paths::parakeet_dir(&app_for_task)?;
                transcriber_parakeet::transcribe_wav(&dir, &wav_for_task, progress)
            }
            "whisper" => {
                let model = paths::model_path(&app_for_task)?;
                transcriber::transcribe_wav(&model, &wav_for_task, &language_for_task, progress)
            }
            engine if transcriber_qwen::is_qwen_engine(engine) => transcriber_qwen::transcribe_wav(
                &app_for_task,
                &wav_for_task,
                engine,
                &language_for_task,
                token_for_task.clone(),
                progress,
            ),
            other => anyhow::bail!("Неизвестный движок транскрибации: {other}"),
        }
    })
    .await;
    ticker_stop.store(true, Ordering::Relaxed);
    if let Some(h) = ticker_handle {
        h.abort();
    }
    let text = text??;
    if token.as_ref().map(|t| t.is_cancelled()).unwrap_or(false) {
        cleanup_wav(&wav_path);
        anyhow::bail!("cancelled");
    }
    let transcribed_in = transcribing_started.elapsed();

    let save_started = Instant::now();
    let out = writer::save_text(&job.save_dir, &stem, &text)?;
    let saved_in = save_started.elapsed();
    cleanup_wav(&wav_path);

    tracing::info!(
        "job {} finished with {engine}: prepared {:.2}s, transcribed {:.2}s, saved {:.2}s",
        job.id,
        seconds(prepared_in),
        seconds(transcribed_in),
        seconds(saved_in)
    );

    Ok((text, out))
}

fn ensure_model_ready(app: &AppHandle, engine: &str) -> Result<()> {
    match engine {
        "parakeet" => {
            if !paths::parakeet_files_ready(app) {
                anyhow::bail!(
                    "Модель Parakeet не скачана. Откройте настройки и нажмите «Скачать модель»."
                );
            }
        }
        "whisper" => {
            let model = paths::model_path(app)?;
            let coreml_ready = paths::coreml_encoder_path(app)
                .map(|p| p.exists())
                .unwrap_or(false);
            if !model.exists() || !coreml_ready {
                anyhow::bail!(
                    "Модель Whisper не скачана. Откройте настройки и нажмите «Скачать модель»."
                );
            }
        }
        engine if transcriber_qwen::is_qwen_engine(engine) => {
            if !transcriber_qwen::is_ready(app, engine) {
                anyhow::bail!(
                    "Qwen MLX еще не готов. Откройте настройки и нажмите «Подготовить модель»."
                );
            }
        }
        other => anyhow::bail!("Неизвестный движок транскрибации: {other}"),
    }
    Ok(())
}

fn emit_error(app: &AppHandle, job: &Job, message: String) {
    let _ = app.emit(
        "job:error",
        JobErrorEvent {
            id: job.id.clone(),
            message,
        },
    );
}

fn emit_canceled(app: &AppHandle, job: &Job) {
    let _ = app.emit("job:canceled", JobCanceledEvent { id: job.id.clone() });
}

fn cleanup_wav(path: &Path) {
    let _ = std::fs::remove_file(path);
}

fn emit_progress(app: &AppHandle, id: &str, stage: &str, percent: u32) {
    let _ = app.emit(
        "job:progress",
        JobProgressEvent {
            id: id.to_string(),
            stage: stage.to_string(),
            percent,
        },
    );
}

fn seconds(duration: Duration) -> f64 {
    duration.as_secs_f64()
}

/// Roughly estimate a 16 kHz mono PCM16 WAV's duration from its file size.
/// The wav converters upstream always produce that exact format.
fn wav_duration_seconds(path: &Path) -> Option<f64> {
    let meta = std::fs::metadata(path).ok()?;
    let bytes = meta.len().saturating_sub(44);
    // 16000 samples/sec × 2 bytes/sample × 1 channel = 32000 bytes/sec
    Some(bytes as f64 / 32_000.0)
}

pub fn new_job_id() -> String {
    Uuid::new_v4().to_string()
}
