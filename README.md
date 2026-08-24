# 🦜 Parrot

> Локальная транскрибация аудио, видео и YouTube-ссылок на macOS + диктовка в любое приложение + конспекты на локальной LLM.

<p align="center">
  <img src="public/parrot.png" alt="Parrot" width="160" />
</p>

Parrot превращает записи разговоров, подкасты, голосовые, видеолекции и YouTube-ссылки в текст прямо на вашем Mac. Всё считается локально на Apple Silicon — аудио не покидает устройство, облачные API не используются.

Разработано Александром Унгуренко в 2026 году.

---

## ✨ Возможности

**Транскрибация**
- 🎙 Drag-and-drop локальных файлов (или ⌘O): аудио `mp3, wav, m4a, flac, ogg, opus, aac, wma`, видео `mp4, mov, mkv, avi, webm, m4v` — всё, что читает ffmpeg.
- 📺 YouTube по ссылке — скачивание и извлечение звука встроенным yt-dlp за один проход.
- 📋 Очередь задач: несколько файлов готовятся параллельно, транскрибация идёт по очереди; прогресс по стадиям и отмена на лету.
- 🌍 Выбор языка: автоопределение, русский, английский, немецкий, французский, испанский, итальянский, португальский, украинский.

**Диктовка**
- 🗣 Hold-to-talk в любом приложении: зажали Alt+Space → говорите → отпустили → текст вставлен в поле ввода. Запись с микрофона, распознавание активным движком, вставка через macOS Accessibility с фолбэком на буфер обмена. Настраиваемое сочетание клавиш, индикация статуса в тулбаре. Требует выдать разрешение Accessibility (приложение само предложит).

**Конспект на локальной LLM**
- 🪶 Одна кнопка «Сгенерировать конспект» под готовым транскриптом: краткое резюме, ключевые темы, важные тезисы и действия — на русском. Сохраняется как `<имя>.summary.md` рядом с транскриптом. Две модели на выбор (см. ниже), окружение ставится автоматически при первом включении.

**История**
- 🕘 Последние 100 транскрипций: поиск, фильтры «Все / С конспектом / YouTube / Файлы», повтор задачи одним кликом (с тем же движком и языком), удаление записей, возврат к старому результату.

**Приложение**
- 🔄 Автообновление: проверка каждые 6 часов, баннер с кнопкой «Установить», ручная проверка в настройках.
- 🎨 Темы оформления: системная / светлая / тёмная.
- 🧭 Онбординг при первом запуске: выбор папки сохранения, режима распознавания и скачивание модели.
- 🔒 Оффлайн-first: никакой телеметрии, аналитики и облачных API. Сеть нужна только для скачивания моделей и загрузки YouTube-роликов.

## 📥 Скачать для Mac

[Скачать Parrot.dmg](https://github.com/ungurenko/parrot/releases/latest/download/Parrot.dmg) — откройте и перетащите Parrot в «Программы».

Или установите одной командой в Терминале:

```bash
curl -fsSL https://raw.githubusercontent.com/ungurenko/parrot/main/install.sh | bash
```

Требования: Mac на Apple Silicon (M1/M2/M3/M4 и новее), macOS 11+. Для диктовки нужно разрешение Accessibility, приложение попросит его само.

## 🧠 Режимы распознавания

В интерфейсе движки представлены как режимы — выбирайте по задаче, техническую модель видно под названием:

| Режим | Модель | Размер | Когда использовать |
|---|---|---|---|
| **Быстро** *(рекомендуется)* | Parakeet V3 (ONNX int8) | ~1.3 ГБ | Встречи, лекции, обычные записи. Быстро, мультиязычный, хорошо работает с русским |
| **Много языков** | Whisper large-v3-turbo (ggml q5_0 + CoreML, Metal) | ~1.2 ГБ | Английский, европейские и редкие языки |
| **Лучше для русского** | Qwen3-ASR 0.6B MLX | ~1.2 ГБ | Когда качество русского текста важнее скорости |
| **Сложная запись** | Qwen3-ASR 1.7B MLX | ~3.4 ГБ | Шум, акценты, тяжёлое аудио; лучше Mac с 32+ ГБ памяти |

- Модели скачиваются один раз через приложение (кнопка «Скачать и выбрать») и хранятся локально; любую можно удалить из настроек.
- Parakeet дополнительно сам ставит MLX-ускоритель (~1 ГБ) на Mac с 16+ ГБ памяти — модель постоянно живёт в памяти и распознавание идёт на GPU (~30× быстрее реального времени, короткие голосовые за доли секунды). На Mac с 8 ГБ остаётся экономичная CPU-версия, при сбое ускорителя Parrot откатывается на неё автоматически.
- На macOS 14+ Parrot сам устанавливает локальное окружение для Qwen3-ASR в Application Support — системный Python и ручные команды не нужны. На macOS 11–13 остаются доступны Parakeet и Whisper, а Qwen показывает понятное ограничение совместимости.

## 🪶 Конспект

| Модель | Размер | Особенности |
|---|---|---|
| **Qwen 3-4B Instruct** (по умолчанию) | ~2.3 ГБ | Стабильный вариант |
| **Gemma 4 E2B-it** | ~3.6 ГБ | Новая модель |

При первом включении Parrot сам скачивает standalone Python и нужные библиотеки в свою папку — системный Python не требуется. Прогресс установки и генерации виден в интерфейсе, генерацию можно отменить.

## ⚙️ Как это работает

```
файл / ссылка
   │
   ▼
ffmpeg: извлечение аудио → WAV 16 kHz mono        (yt-dlp + ffmpeg для YouTube)
   │
   ▼
движок распознавания (Parakeet / Qwen3-ASR / Whisper)
   │
   ▼
<имя файла>.txt          ← транскрипт, папка по умолчанию ~/Documents/Transcripts
<имя файла>.summary.md   ← конспект (если сгенерирован)
```

## 🔐 Приватность и данные

- Аудио и видео обрабатываются только локально, файлы не загружаются в облако.
- Нет аналитики, трекеров и сторонних API.
- Все данные живут в `~/Library/Application Support/com.alexk.parrot/`: модели, настройки (`settings.json`), история (`history.json`), Python-окружение для конспекта, логи. Транскрипты сохраняются в выбранную вами папку (по умолчанию `~/Documents/Transcripts`). Приложение можно полностью удалить вместе с этой папкой.

## 🛠 Технологический стек

- **Desktop:** Tauri 2.0 (Rust + WebView), macOS arm64, минимум macOS 11.
- **Backend:** Rust — `whisper-rs` (Metal + CoreML), `parakeet-rs` (ONNX), `tokio`, `cpal` (запись микрофона), `arboard` (буфер обмена), libc (SIGTERM для отмены задач).
- **MLX-подсистема:** Python venv в пользовательской папке — `mlx-lm` / `mlx-vlm` для конспекта и `mlx-qwen3-asr` для Qwen-режимов.
- **Frontend:** React 19 + TypeScript + Vite + Tailwind v4 + shadcn/ui.
- **Sidecar-бинарники:** бандл `ffmpeg` и `yt-dlp` внутри `.app`.
- **Плагины Tauri:** updater, global-shortcut, single-instance, dialog, opener, process.

## 🏗 Архитектура

Полный контекст для ИИ-агентов — [`AGENTS.md`](AGENTS.md) / [`CLAUDE.md`](CLAUDE.md) и правила в [`.claude/rules/`](.claude/rules/) (`binaries.md` — бандл sidecar'ов, `models.md` — модели и скачивание, `tauri.md` — IPC и capabilities). Ниже — краткая карта.

### Rust (`src-tauri/src/`)

| Модуль | Назначение |
|--------|------------|
| `lib.rs` | Entry, регистрация IPC-команд, preload движка, тикер прогресса саммари, cleanup при закрытии окна |
| `queue.rs` | FIFO-очередь: 2 prep-воркера + слот активной транскрибации, диспатч по движку, cancel-проверки |
| `cancellation.rs` | `CancelRegistry` + race-safe `CancelToken` (atomic flag + PID-регистрация для SIGTERM) |
| `source/local.rs` · `source/youtube.rs` | Извлечение аудио через ffmpeg / yt-dlp |
| `transcriber_parakeet.rs` | Parakeet V3: ONNX int8 (чанки 5 мин + 5 c overlap) + опциональный MLX-ускоритель |
| `transcriber.rs` | Whisper large-v3-turbo (whisper-rs, Metal + CoreML) |
| `transcriber_qwen.rs` | Qwen3-ASR MLX: warm server на localhost с fallback на CLI |
| `summarizer_qwen3.rs` · `summarizer_models.rs` | Конспект через `mlx_lm`/`mlx_vlm`; спецификации моделей, warm OpenAI-совместимый сервер |
| `dictation.rs` | Hold-to-talk: запись с cpal, глобальный хоткей, вставка текста через Accessibility API |
| `history.rs` | История транскрипций (до 100 записей) |
| `mlx_env.rs` | Автоустановка Python-окружения для конспекта (standalone Python + venv + пакеты) |
| `model.rs` | Скачивание моделей с прогрессом |
| `paths.rs` · `settings.rs` | Пути в Application Support; JSON-настройки с валидацией |
| `writer.rs` | Сохранение `<stem>.txt` и `<stem>.summary.md` |
| `binaries.rs` | Резолв sidecar ffmpeg/yt-dlp в dev и prod |

### React (`src/`)

| Компонент | Назначение |
|-----------|------------|
| `App.tsx` | Роутинг онбординг ↔ главное окно, состояние задач |
| `components/DropZone.tsx` · `YouTubeInput.tsx` | Ввод файлов и ссылок |
| `components/JobList.tsx` · `ProcessingView.tsx` | Очередь и экран обработки |
| `components/ResultView.tsx` · `SummaryPanel.tsx` | Результат + конспект |
| `components/HistoryList.tsx` | История с поиском и фильтрами |
| `components/EnginePicker.tsx` | Выбор режима, скачивание/удаление моделей |
| `components/SettingsModal.tsx` | Настройки: модель, язык, диктовка, тема, папка, обновления, конспект |
| `components/Onboarding.tsx` | Первый запуск |
| `components/UpdateBanner.tsx` · `UpdateChecker.tsx` | Автообновление |
| `hooks/useJobEvents.ts` | Подписка на события Rust → UI |

### IPC-команды (Tauri)

`enqueue_file(path)` · `enqueue_youtube(url)` · `cancel_job(id)` · `get_settings` / `set_settings` · `is_model_ready` · `get_engine_statuses` · `download_model` · `download_model_for_engine(engine)` · `delete_model_for_engine(engine)` · `get_summarizer_status` · `setup_summarizer_env` · `download_summarizer_model` / `delete_summarizer_model` · `summarize(id, transcript, transcript_path)` · `cancel_summary(id)` · `get_dictation_status` · `check_accessibility_permission` / `request_accessibility_permission` · `get_history` / `load_history_entry(id)` / `delete_history_entry(id)` / `clear_history` · `open_in_finder(path)` · `open_logs` · `log_client_error(scope, message)`

Защита: `transcript_path` для саммари обязан быть `.txt` внутри `save_dir` (canonicalize обеих сторон); смена/удаление модели заблокированы при активной задаче.

### События (Rust → фронт)

- `job:*` — `queued`, `progress` (стадии `preparing|extracting|downloading|transcribing`), `title`, `done {text, outputPath}`, `error`, `canceled`
- `summary:*` — `progress` (стадии `loading|generating|finalizing`), `done`, `error`, `canceled`
- `model:*` / `summary_model:*` — `progress` (процент, байты, скорость), `stage downloading|warmup|ready`
- `summary_env:progress` — шаги установки окружения конспекта
- `parakeet_mlx:progress/:ready/:error` — установка MLX-ускорителя Parakeet
- `dictation:started/:processing/:done/:error` — фазы диктовки
- `history:updated` — изменилась история

### Настройки (`settings.json`)

`save_dir` · `onboarded` · `engine` (parakeet | whisper | qwen-0.6b | qwen-1.7b) · `language` (auto + 8 языков) · `summarizer_enabled` · `summary_model` (qwen3-4b | gemma4-e2b) · `dictation_enabled` · `dictation_hold_key` (по умолчанию Alt+Space) · `theme` (system | light | dark)

## 👨‍💻 Разработка

Требования: Node.js 20+, Rust toolchain (см. `rust-toolchain.toml`), Xcode Command Line Tools.

```bash
npm run tauri dev      # запуск приложения с hot-reload
npm run check          # тесты UI + сборка фронта + cargo test/fmt/clippy
npm run test:ui        # только vitest
npx tsc --noEmit       # только TypeScript
npm run build:local    # локальная сборка .app + .dmg без updater-подписи
npm run release:mac    # релизная сборка: подпись, repack, latest.json, загрузка
```

Релизный конвейер — скрипты в `tools/` (`release_mac.mjs`, `sign_macos_bundle.sh`, `repack_macos_release.mjs`, `generate_latest_json.mjs`, `upload_release.mjs`); ключ updater'а лежит в `~/.tauri/parrot.key`.

⚠️ Никогда не удаляйте `src-tauri/binaries/` — там бандл ffmpeg и yt-dlp, восстанавливается только ручным скачиванием (см. `.claude/rules/binaries.md`).

### Документация

| Файл | О чём |
|------|-------|
| [`AGENTS.md`](AGENTS.md) / [`CLAUDE.md`](CLAUDE.md) | Полный архитектурный контекст для агентов |
| [`.claude/rules/models.md`](.claude/rules/models.md) | URL, размеры и логика скачивания моделей |
| [`.claude/rules/binaries.md`](.claude/rules/binaries.md) | Бандлинг и восстановление ffmpeg/yt-dlp |
| [`.claude/rules/tauri.md`](.claude/rules/tauri.md) | IPC, capabilities, sidecar-конвенции |
| [`docs/updates.md`](docs/updates.md) | Схема updater'а и чеклист публикации |
| [`docs/qwen-mlx.md`](docs/qwen-mlx.md) | Dev-окружение Qwen3-ASR |
| [`docs/local-summary-models.md`](docs/local-summary-models.md) | Модели конспекта, версии MLX, probe-скрипт |
| [`docs/release-notes.md`](docs/release-notes.md) | Заметки последнего релиза |
| [`docs/for-users-install-and-update.md`](docs/for-users-install-and-update.md) | Выдача DMG пользователям |

## 🔄 Обновления

Parrot проверяет GitHub Releases каждые 6 часов и предлагает установить версию одной кнопкой (endpoint — `latest.json` с minisign-подписью). Подробности — в [`docs/updates.md`](docs/updates.md).

## 📄 Автор

© 2026 Александр Унгуренко.
