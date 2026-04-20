# 🦜 Parrot

> Локальная транскрибация аудио, видео и YouTube-ссылок на macOS.

<p align="center">
  <img src="public/parrot.png" alt="Parrot" width="160" />
</p>

Parrot превращает записи разговоров, подкасты, голосовые, видеолекции и YouTube-ссылки в текст прямо на вашем Mac. Всё считается локально на Apple Silicon — аудио не покидает устройство, облачные API не используются.

Разработано Александром Унгуренко в 2026 году.

## ✨ Возможности

- 🎙 Drag-and-drop локальных аудио- и видеофайлов (MP3, M4A, WAV, MP4, MOV и других форматов, которые читает ffmpeg).
- 📺 Транскрибация YouTube по ссылке — встроенный yt-dlp.
- 🧠 Четыре движка на выбор: Parakeet V3, Whisper large-v3-turbo, Qwen3-ASR 0.6B и 1.7B (MLX).
- 📋 Очередь задач с прогрессом и отменой на лету.
- 🌍 Поддержка русского, английского и многоязычных моделей.
- 🔒 Оффлайн-first: приватность локального инструмента, никакой телеметрии.

## 📥 Скачать для Mac

[Скачать Parrot.dmg](https://github.com/ungurenko/parrot/releases/latest/download/Parrot.dmg)

Откройте скачанный файл и перетащите Parrot в папку «Программы».

Подходит для Mac с Apple Silicon: M1, M2, M3, M4 и новее.

## 🧠 Движки транскрибации

| Движок | Модель | Когда использовать |
|---|---|---|
| **Parakeet V3** (по умолчанию) | ONNX int8, encoder + decoder-joint | Быстрая транскрибация на английском, лёгкая по памяти |
| **Whisper large-v3-turbo** | `ggml-large-v3-turbo-q5_0` + CoreML-encoder, Metal | Многоязычные записи, когда важно качество |
| **Qwen3-ASR 0.6B** | MLX-версия `Qwen/Qwen3-ASR-0.6B` | Русский и китайский при ограниченной памяти |
| **Qwen3-ASR 1.7B** | MLX-версия `Qwen/Qwen3-ASR-1.7B` | Максимальное качество на русском и мультиязыке |

Модели скачиваются один раз при первом запуске и хранятся в `~/Library/Application Support/com.alexk.parrot/`.

## 🔐 Приватность

- Аудио и видео обрабатываются локально, файлы не загружаются в облако.
- Сеть нужна только для скачивания моделей и загрузки YouTube-роликов.
- В приложении нет аналитики, трекеров и сторонних API.

## 🛠 Технологический стек

- **Desktop:** Tauri 2.0 (Rust + WebView), macOS arm64.
- **Backend:** Rust — `whisper-rs`, `parakeet-rs`, `tokio`, libc (SIGTERM для отмены задач).
- **Frontend:** React 19 + TypeScript + Vite + Tailwind v4 + shadcn/ui.
- **Sidecar-бинарники:** бандл `ffmpeg` и `yt-dlp` для извлечения и загрузки аудио.

## 👨‍💻 Разработка

Требования: Node.js 20+, Rust toolchain (см. `rust-toolchain.toml`), Xcode Command Line Tools.

- `npm run tauri dev` — запуск приложения в режиме разработки с hot-reload.
- `npm run build:local` — локальная сборка `.app` и `.dmg` без подписи обновлений.
- `npm run release:mac` — релизная сборка с подписью updater и генерацией `latest.json`.
- `npm run check` — проверка фронтенда, Rust-тестов, форматирования и Clippy.

Подробности архитектуры — в `CLAUDE.md` и модульных правилах в `.claude/rules/`.

## 🔄 Обновления

Parrot проверяет обновления через GitHub Releases с updater-подписью. Подробности — в [`docs/updates.md`](docs/updates.md).

## 📄 Автор

© 2026 Александр Унгуренко.
