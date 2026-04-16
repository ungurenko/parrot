# Parrot

Локальное macOS-приложение для транскрибации аудио/видео/YouTube в текст. Tauri 2 + Rust + React.
Разработано Александром Унгуренко в 2026 году.

## Критические правила

- **Никогда не удаляй `src-tauri/binaries/`** — там бандл ffmpeg и yt-dlp для sidecar, восстановить можно только ручным скачиванием (см. `.claude/rules/binaries.md`).
- **Не меняй sidecar-названия** — `ffmpeg-aarch64-apple-darwin` и `yt-dlp-aarch64-apple-darwin`, без суффикса Tauri не найдёт.
- **Весь код оффлайн-first** — любая транскрибация должна работать без сети (кроме скачивания модели и YouTube).
- **Не добавляй аналитику, телеметрию, cloud-API** — приватность локального инструмента.

## Команды

- `npm run tauri dev` — dev-режим с hot-reload
- `npm run build:local` — локальная сборка `.app` + `.dmg` без updater-подписи
- `npm run release:mac` — релизная сборка с updater-подписью и свежим `latest.json`
- `npm run check` — frontend build + Rust tests/fmt/clippy
- `cargo check` (из `src-tauri/`) — быстрая проверка Rust
- `npx tsc --noEmit` — проверка TypeScript

## Архитектура

| Путь | Назначение |
|------|------------|
| `src-tauri/src/lib.rs` | Entry, IPC-команды (enqueue, cancel_job, download_model), preload активного движка |
| `src-tauri/src/queue.rs` | Tokio FIFO-очередь, диспатч по engine, cancel-проверки, ticker прогресса для Qwen |
| `src-tauri/src/cancellation.rs` | `CancelRegistry` + `CancelToken` с atomic-флагом и PID-регистрацией для SIGTERM |
| `src-tauri/src/source/` | Извлечение аудио: `local.rs` (ffmpeg), `youtube.rs` (yt-dlp) |
| `src-tauri/src/transcriber.rs` | Whisper engine (whisper-rs + Metal + CoreML) |
| `src-tauri/src/transcriber_parakeet.rs` | Parakeet V3 engine (parakeet-rs, ONNX, int8, 8 потоков) |
| `src-tauri/src/model.rs` | Скачивание моделей с прогрессом |
| `src-tauri/src/paths.rs` | Пути к моделям/настройкам/логам через Tauri API |
| `src-tauri/src/settings.rs` | JSON-настройки (save_dir, engine, onboarded) |
| `src-tauri/src/binaries.rs` | Резолв sidecar ffmpeg/yt-dlp в dev и prod |
| `src/App.tsx` | Главное окно, роутинг онбординг ↔ main |
| `src/components/` | DropZone, YouTubeInput, JobList, ResultView, SettingsModal, Onboarding, EnginePicker |
| `src/hooks/useJobEvents.ts` | Подписка на job:queued/progress/done/error |

## Ключевые паттерны

- **Движки транскрибации** — четыре реализации (Parakeet, Whisper, Qwen3-ASR 1.7B/0.6B MLX), выбор через `settings.engine`, диспатч в очереди. Дефолт — `parakeet`; Qwen показывается как недоступный, если CLI `mlx-qwen3-asr` не установлен.
- **Отмена задач** — IPC `cancel_job(id)` ставит atomic-флаг + шлёт `SIGTERM` зарегистрированным PID (ffmpeg / yt-dlp / mlx-qwen3-asr). Отменённые джобы эмитят `job:canceled`, а фронт показывает отдельный статус `canceled`.
- **Прогресс Qwen CLI** — CLI не выдаёт intermediate-прогресс. В `queue.rs` запускается async-ticker, который оценивает длительность WAV (файл всегда 16 kHz mono s16, поэтому `len / 32000`) и тикает 5→95% по elapsed × RTF (0.45 для 0.6B, 0.9 для 1.7B).
- **Прогресс подготовки модели** — `download_model` для Qwen поллит размер `qwen-cache` относительно `EXPECTED_QWEN_*_BYTES`. События: `model:progress` (0–100) и `model:stage` (`downloading` → `warmup` → `ready`). Фронт показывает разный текст и пульсацию в стадии warmup.
- **Проверка полноты модели Qwen** — `transcriber_qwen::model_cache_exists` сравнивает размер кеша с ожидаемым (≥90%). Защищает от запуска транскрибации при частично скачанной модели.
- **Sidecar binaries** — ffmpeg и yt-dlp бандлятся в `.app/Contents/Resources/`, в Rust вызываются через `tokio::process::Command` с путём из `binaries::resolve_sidecar`.
- **IPC** — команды через `tauri::command`, события от Rust (`job:progress`, `model:progress`, `job:done`, `job:error`) через `AppHandle::emit`.
- **Progress pipeline** — stages `downloading`/`extracting`/`transcribing` с процентами, парсинг прогресса yt-dlp из `--progress-template`.
- **Чанкование для Parakeet TDT** — ONNX-граф имеет лимит ~8-10 мин; в `transcriber_parakeet` режем на 5-мин чанки с 5-сек overlap.

## Модели

| Engine | Файлы | Путь |
|--------|-------|------|
| Qwen3-ASR 0.6B | HF-модель `Qwen/Qwen3-ASR-0.6B`, CLI `mlx-qwen3-asr` | `…/qwen-cache/` (HF_HOME), CLI в `.qwen-mlx/venv/bin/` |
| Qwen3-ASR 1.7B | HF-модель `Qwen/Qwen3-ASR-1.7B` | там же |
| Parakeet | `encoder-model.int8.onnx`, `decoder_joint-model.int8.onnx`, `vocab.txt` | `…/models/parakeet-v3/` |
| Whisper | `ggml-large-v3-turbo-q5_0.bin` + `ggml-large-v3-turbo-encoder.mlmodelc/` | `~/Library/Application Support/com.alexk.parrot/models/` |

Детали скачивания и URL — в `.claude/rules/models.md`.

## Модульные правила

См. `.claude/rules/`:
- `models.md` — URL, размеры, логика скачивания моделей
- `binaries.md` — бандлинг ffmpeg/yt-dlp, восстановление
- `tauri.md` — IPC, capabilities, sidecar-конвенции

## Технический стек

Tauri 2.0, Rust (whisper-rs 0.16 + parakeet-rs 0.3 + tokio + libc для SIGTERM), React 19 + TS + Vite + Tailwind v4, macOS arm64.
