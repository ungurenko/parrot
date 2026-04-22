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

## Code signing (entitlements)

`yt-dlp_macos` — PyInstaller one-file: на старте распаковывает `Python.framework` в `/var/folders/.../_MEIxxxx/` и делает `dlopen`. Под hardened runtime это падает с `different Team IDs`, потому что embedded Python.framework подписан yt-dlp-автором, а сам процесс — ad-hoc подписью от Tauri.

Фикс: `src-tauri/entitlements.plist` подключён через `bundle.macOS.entitlements`. Ключи:
- `com.apple.security.cs.disable-library-validation` — разрешает загрузку dylib с другим Team ID (главный ключ).
- `com.apple.security.cs.allow-dyld-environment-variables` — PyInstaller использует `_PYI_*` env-переменные.
- `com.apple.security.cs.allow-unsigned-executable-memory` + `com.apple.security.cs.allow-jit` — страховка под CPython extensions.

**Проверка после сборки:**
```bash
codesign -d --entitlements - src-tauri/target/release/bundle/macos/Parrot.app/Contents/MacOS/yt-dlp
```
Должны быть видны оба ключа. Если sidecar не унаследовал entitlements — пере-подписать вручную:
```bash
codesign --force --sign - --entitlements src-tauri/entitlements.plist --options runtime \
  src-tauri/target/release/bundle/macos/Parrot.app/Contents/MacOS/yt-dlp
codesign --force --sign - --entitlements src-tauri/entitlements.plist --options runtime \
  src-tauri/target/release/bundle/macos/Parrot.app
```
(sidecar первым, потом сам `.app`, чтобы его подпись охватила уже подписанный ресурс).

## YouTube download

yt-dlp вызывается с `-f bestaudio/best` + `--newline --progress-template "PROGRESS %(progress._percent_str)s"`. Сырое аудио (m4a/opus/webm) скачивается без ремукса, ffmpeg в один проход делает 16 kHz mono WAV.

**Не используй `-x --audio-format wav`** — это даёт двойную конвертацию и раздувает промежуточный файл в 10× (был баг, не возвращаем).
