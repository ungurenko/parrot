---
paths: "src-tauri/src/{model,paths,transcriber*}.rs"
---

# Модели транскрибации

## Whisper Large-v3 turbo (fallback для азиатских языков)

- **GGUF:** `https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin` (~550 МБ)
- **CoreML encoder:** `https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-encoder.mlmodelc.zip` (~660 МБ, распаковать через `unzip` в ту же папку)
- whisper.cpp при поиске CoreML-файла стрипает `-qN_N` суффикс → ожидает `ggml-large-v3-turbo-encoder.mlmodelc` (без `-q5_0`)
- Metal и CoreML активируются через feature flags `whisper-rs = { features = ["metal", "coreml"] }`

## Qwen 3-4B Instruct (саммарайзер)

- **Модель:** `mlx-community/Qwen3-4B-Instruct-2507-4bit` (~2.3 ГБ), через `mlx_lm.generate` Python-модуль.
- **Cache:** `paths::qwen_cache_dir(app)` (тот же HF_HOME, что у Qwen ASR).
- **Python:** standalone cpython 3.12.13 от `astral-sh/python-build-standalone`, качается приложением при первом нажатии «Установить окружение». URL и SHA256 захардкожены в `summarizer_qwen3.rs::STANDALONE_PYTHON_URL/SHA256`. Для апдейта версии: pick новый tag в `https://github.com/astral-sh/python-build-standalone/releases`, возьми SHA256 из `SHA256SUMS` того же релиза, замени **обе** константы вместе.
- **Venv:** `paths::qwen_env_dir` (`…/Application Support/com.alexk.parrot/.qwen-mlx/venv/`), создаётся через standalone Python; ставится только `mlx-lm>=0.24.0` (минимальная зависимость для саммари).
- **Команда:** `setup_summarizer_env` Tauri IPC → `summarizer_qwen3::install_env` (идемпотентная). События прогресса — `summary_env:progress` со строками.

## Qwen3-ASR 0.6B / 1.7B MLX

- **Модели:** `Qwen/Qwen3-ASR-1.7B` и `Qwen/Qwen3-ASR-0.6B` на HuggingFace (MLX-порт, нативно на Apple Silicon).
- **CLI:** `.qwen-mlx/venv/bin/mlx-qwen3-asr`, ищется также через `PARROT_QWEN_BIN`, старый `AUDIO_TO_TEXT_QWEN_BIN` и `$PATH` (см. `transcriber_qwen::resolve_cli`).
- **Установка:** `tools/setup_qwen_mlx.sh` создаёт venv и ставит `mlx-qwen3-asr`.
- **Cache:** `HF_HOME` принудительно указывает на `paths::qwen_cache_dir(app)` (`~/Library/Application Support/com.alexk.parrot/models/qwen-mlx/`).
- **Флаги запуска:** `--stdout-only --no-progress --model <repo>`.
- **Warmup:** `transcriber_qwen::warmup_model` прогоняет 1 сек тишины, чтобы HF скачал модель.
- **Длинное аудио:** 1.7B ест ~6–7 GiB RAM на 2-минутных фрагментах (M3 Max). На 16 GB Mac для длинных файлов — использовать 0.6B или Parakeet.
- **Качество:** SOTA по WER на мультиязыке, контекстно-осознанная пунктуация, 30 языков + 22 китайских диалекта.

## Parakeet TDT v3 (альтернатива — максимальная скорость, 25 языков)

- **Репозиторий:** `istupakov/parakeet-tdt-0.6b-v3-onnx` на HuggingFace
- **Файлы int8 (по умолчанию):**
  - `encoder-model.int8.onnx` — ~300 МБ
  - `decoder_joint-model.int8.onnx` — ~20 МБ
  - `vocab.txt`
- **fp32 (для лучшего качества, если int8 не подходит):**
  - `encoder-model.onnx` + `encoder-model.onnx.data` (~1.2 ГБ total)
  - `decoder_joint-model.onnx` (~72 МБ)
- parakeet-rs автоматически находит файлы в порядке: `encoder-model.onnx` → `encoder.onnx` → `encoder-model.int8.onnx`. Для форсирования int8 — удалить fp32 файлы.
- **Ограничение TDT:** ~8-10 мин per inference call. Длинное аудио режем на 5-мин чанки с 5-сек overlap (см. `transcriber_parakeet.rs`).
- **Execution provider:** только CPU работает надёжно. CoreML EP медленнее CPU для Parakeet (dynamic shapes). WebGPU экспериментален.
- **Threads:** `intra_threads = min(available_parallelism, 8)`, `inter_threads = 1`.

## Логика выбора движка

- `Settings.engine: "parakeet" | "whisper" | "qwen-0.6b" | "qwen-1.7b"`, дефолт `parakeet` (стабильный путь для обычного пользователя).
- Qwen показывается как недоступный, если CLI `mlx-qwen3-asr` не найден.
- `is_model_ready(app)` проверяет файлы активного движка (для Qwen — существование HF cache и CLI).
- `download_model(app)` скачивает активный движок (для Qwen — вызывает `warmup_model`, который триггерит HF-скачивание).
- `preload_active_engine(handle)` в `setup()` прогревает Parakeet/Whisper в фоне. Для Qwen preload пропускается — CLI стартует per-job.

## Прогресс-модель

- Событие `model:progress` с `u32` (0-100).
- `download_with_progress(url, dest, percent_start, percent_end)` эмитит линейный прогресс в пределах весов.
