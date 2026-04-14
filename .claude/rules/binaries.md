---
paths: "src-tauri/{binaries,src/binaries.rs,src/source/**,tauri.conf.json}"
---

# Бандл-бинарники (ffmpeg + yt-dlp)

## Расположение

`src-tauri/binaries/`:
- `ffmpeg-aarch64-apple-darwin` (~50 МБ, статический для arm64)
- `yt-dlp-aarch64-apple-darwin` (~37 МБ, standalone)

## Соглашение именования

Tauri-sidecar требует суффикс `-<target-triple>`. Для Apple Silicon — `aarch64-apple-darwin`. Tauri стрипает суффикс при бандлинге — в итоговом `.app/Contents/Resources/` лежат просто `ffmpeg` и `yt-dlp`.

## Конфигурация

В `tauri.conf.json`:
```json
"bundle": {
  "externalBin": ["binaries/ffmpeg", "binaries/yt-dlp"]
}
```

## Резолв пути в коде

`src-tauri/src/binaries.rs::resolve_sidecar()`:
1. Сначала пытается `app.path().resolve(name, Resource)` — работает в prod.
2. Fallback — `CARGO_MANIFEST_DIR/binaries/<name>-<triple>` для dev.

## Восстановление при потере

```bash
cd src-tauri/binaries
curl -sL -o yt-dlp-aarch64-apple-darwin \
  "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos"
chmod +x yt-dlp-aarch64-apple-darwin

curl -sL -o ffmpeg_arm.zip "https://www.osxexperts.net/ffmpeg71arm.zip"
unzip -o ffmpeg_arm.zip && rm ffmpeg_arm.zip __MACOSX -rf
mv ffmpeg ffmpeg-aarch64-apple-darwin && chmod +x ffmpeg-aarch64-apple-darwin
```

## Системные зависимости для сборки

- `cmake` (для whisper-rs metal build): `brew install cmake`
- Xcode Command Line Tools (для CoreML)

## YouTube download

yt-dlp вызывается с `-f bestaudio/best` + `--newline --progress-template "PROGRESS %(progress._percent_str)s"`. Сырое аудио (m4a/opus/webm) скачивается без ремукса, ffmpeg в один проход делает 16 kHz mono WAV.

**Не используй `-x --audio-format wav`** — это даёт двойную конвертацию и раздувает промежуточный файл в 10× (был баг, не возвращаем).
