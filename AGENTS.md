# Parrot

Локальное macOS-приложение для транскрибации аудио/видео/YouTube в текст + опциональный локальный LLM-конспект. Tauri 2 + Rust + React.
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
| `src-tauri/src/lib.rs` | Entry и setup-проводка; здесь остались только `get_settings`/`set_settings`, `get_dictation_status` и accessibility-команды — остальные IPC-команды переехали в `commands/`. Preload активного движка, `cancel_all` на закрытии окна |
| `src-tauri/src/commands/` | IPC-команды по доменам: `jobs.rs` (enqueue/cancel), `models.rs` (статусы/скачивание/удаление моделей), `summary.rs` (саммари + его ticker прогресса), `history.rs`, `system.rs`; в `mod.rs` — общий warmup-скаффолд `run_model_warmup` и poller прогресса |
| `src-tauri/src/queue.rs` | Tokio FIFO-очередь, диспатч по engine, cancel-проверки, ticker прогресса для Qwen |
| `src-tauri/src/cancellation.rs` | `CancelRegistry` + `CancelToken` с atomic-флагом и PID-регистрацией для SIGTERM. `try_create` — атомарное создание без перезаписи. `cancel_all` — массовая отмена (используется на close) |
| `src-tauri/src/dictation.rs` | Диктовка: запись с удержанием шортката, вставка текста, global shortcut, статус `{phase}` |
| `src-tauri/src/history.rs` | История транскрипций (append/read/delete, лимит 100 записей) |
| `src-tauri/src/mlx_env.rs` | Общая MLX-инфраструктура: standalone Python + venv, install lock, `pip_install`/`python_import_ok`, HTTP-константы и health warm-серверов, `stop_child`, `candidate_roots`, проверка полноты HF-кэша |
| `src-tauri/src/hardware.rs` | Probe железа Mac (RAM через sysctl) — выбор безопасного режима ускорения |
| `src-tauri/src/fs_metrics.rs` | `dir_size_bytes` — размер кеша моделей для прогресса скачивания |
| `src-tauri/src/source/` | Извлечение аудио: `local.rs` (ffmpeg), `youtube.rs` (yt-dlp) |
| `src-tauri/src/transcriber.rs` | Whisper engine (whisper-rs + Metal + CoreML) |
| `src-tauri/src/transcriber_parakeet.rs` | Parakeet V3 engine (parakeet-rs, ONNX, int8, 8 потоков) |
| `src-tauri/src/transcriber_qwen.rs` | Qwen3-ASR MLX engine (subprocess + warm server) |
| `src-tauri/src/summarizer_qwen3.rs` | Локальный LLM-конспект через `mlx_lm.generate` (subprocess из user-space venv). `install_env` сам качает standalone Python от astral-sh + ставит mlx-lm — без системных зависимостей. Тот же HF_HOME, что и ASR |
| `src-tauri/src/prompts.rs` | Системный и user-промпт для конспекта на русском |
| `src-tauri/src/model.rs` | Скачивание моделей с прогрессом |
| `src-tauri/src/paths.rs` | Пути к моделям/настройкам/логам через Tauri API |
| `src-tauri/src/settings.rs` | JSON-настройки (save_dir, engine, language, onboarded, summarizer_enabled, summary_model, theme, dictation_enabled, dictation_hold_key) |
| `src-tauri/src/writer.rs` | `save_text` для .txt транскрипта и `save_summary` для `<stem>.summary.md` рядом |
| `src-tauri/src/binaries.rs` | Резолв sidecar ffmpeg/yt-dlp в dev и prod |
| `src/App.tsx` | Главное окно, роутинг онбординг ↔ main |
| `src/components/` | DropZone, YouTubeInput, JobList, ResultView, SettingsModal, Onboarding, EnginePicker, SummaryPanel |
| `src/hooks/useJobEvents.ts` | Подписка на job:* и summary:* события |

## Ключевые паттерны

- **Движки транскрибации** — четыре реализации (Parakeet, Whisper, Qwen3-ASR 1.7B/0.6B MLX), выбор через `settings.engine`, диспатч в очереди. Дефолт — `parakeet`; Qwen показывается как недоступный, если CLI `mlx-qwen3-asr` не установлен.
- **Отмена задач** — IPC `cancel_job(id)` ставит atomic-флаг + шлёт `SIGTERM` зарегистрированным PID (ffmpeg / yt-dlp / mlx-qwen3-asr / mlx_lm). Саммари отменяется через `cancel_summary(id)` против отдельного `summary_cancel: CancelRegistry` в `AppState`. Отменённые эмитят `job:canceled` / `summary:canceled`.
- **Race-safe CancelToken** — `cancel()` держит лок pids при переключении флага; `register_pid()` под тем же локом проверяет флаг и шлёт SIGTERM сразу, если cancel уже был. `try_create()` атомарно отказывает в повторном старте для того же id.
- **Прогресс Qwen CLI** — CLI не выдаёт intermediate-прогресс. В `queue.rs` запускается async-ticker, который оценивает длительность WAV (файл всегда 16 kHz mono s16, поэтому `len / 32000`) и тикает 5→95% по elapsed × RTF (0.45 для 0.6B, 0.9 для 1.7B).
- **Прогресс саммарайзера** — собственный ticker в `commands/summary.rs::summarize`: 0→95% по оценке `4s load + expected_tokens/60 tok/s`. Фазы `loading` → `generating` → `finalizing` (100% при завершении).
- **Прогресс подготовки модели** — `download_model_for_engine` / `download_summarizer_model` поллит размер кеш-директории относительно ожидаемых байт через poller из `commands/mod.rs`. События: `model:progress` + `model:stage` (для ASR) и `summary_model:progress` + `summary_model:stage` (для саммарайзера). Стадии `downloading` → `warmup` → `ready`.
- **Проверка полноты моделей** — общий `mlx_env::model_cache_ready` сравнивает размер HF-кеша с ожидаемым (≥90%); обёртки в `transcriber_qwen::model_cache_ready` и `summarizer_qwen3::model_cache_exists` добавляют ready-marker. Защищает от запуска при частично скачанной модели.
- **Cleanup на закрытии окна** — в `setup()` при `CloseRequested` вызываются `binaries::stop_yt_dlp_startup_cache()` + `transcriber_qwen::stop_server()` + `summary_cancel.cancel_all()` (убивает все in-flight Python-процессы).
- **Sidecar binaries** — ffmpeg и yt-dlp бандлятся в `.app/Contents/Resources/`, в Rust вызываются через `tokio::process::Command` с путём из `binaries::resolve_sidecar`.
- **IPC** — команды через `tauri::command`, события от Rust (`job:*`, `model:*`, `summary:*`, `summary_model:*`) через `AppHandle::emit` с `#[serde(rename_all = "camelCase")]`.
- **Progress pipeline** — stages `downloading`/`extracting`/`transcribing` с процентами, парсинг прогресса yt-dlp из `--progress-template`.
- **Чанкование для Parakeet TDT** — ONNX-граф имеет лимит ~8-10 мин; в `transcriber_parakeet` режем на 5-мин чанки с 5-сек overlap.

## Модели

| Engine | Файлы | Путь |
|--------|-------|------|
| Qwen3-ASR 0.6B | HF-модель `Qwen/Qwen3-ASR-0.6B`, CLI `mlx-qwen3-asr` | `…/qwen-mlx/` (HF_HOME), CLI в `.qwen-mlx/venv/bin/` |
| Qwen3-ASR 1.7B | HF-модель `Qwen/Qwen3-ASR-1.7B` | там же |
| Qwen 3-4B Instruct (саммари) | HF-модель `mlx-community/Qwen3-4B-Instruct-2507-4bit`, через `mlx_lm` Python-модуль | тот же `…/qwen-mlx/` (HF_HOME). Standalone Python в `…/.qwen-mlx/python/bin/python3.12`, venv в `…/.qwen-mlx/venv/bin/python` (всё в Application Support) |
| Parakeet | `encoder-model.int8.onnx`, `decoder_joint-model.int8.onnx`, `vocab.txt` | `…/models/parakeet-v3/` |
| Whisper | `ggml-large-v3-turbo-q5_0.bin` + `ggml-large-v3-turbo-encoder.mlmodelc/` | `~/Library/Application Support/com.alexk.parrot/models/` |

Venv для саммари (`mlx-lm>=0.24.0`) создаётся прямо из приложения через `setup_summarizer_env` — Python тоже скачивается автоматически (см. паттерн «Автоустановка окружения для саммарайзера»). Для Qwen ASR-движков (CLI `mlx-qwen3-asr[serve]`) пока работает только dev-флоу через `tools/setup_qwen_mlx.sh` — у DMG-пользователей Qwen ASR показывается недоступным.

Детали скачивания и URL — в `.claude/rules/models.md`.

## Модульные правила

См. `.claude/rules/`:
- `models.md` — URL, размеры, логика скачивания моделей
- `binaries.md` — бандлинг ffmpeg/yt-dlp, восстановление
- `tauri.md` — IPC, capabilities, sidecar-конвенции

## Технический стек

Tauri 2.0, Rust (whisper-rs 0.16 + parakeet-rs 0.3 + tokio + libc для SIGTERM), React 19 + TS + Vite + Tailwind v4, Python 3.12 venv для MLX (Qwen3-ASR + mlx-lm), macOS arm64.
