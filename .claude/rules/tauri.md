---
paths: "src-tauri/{capabilities/**,tauri.conf.json,src/lib.rs}"
---

# Tauri 2 specifics

## Async runtime

**Не используй `tokio::spawn` в `setup()` и других context-free местах** — Tokio runtime там не доступен, будет паника `tokio::runtime::Handle::current()`.

Вместо этого — `tauri::async_runtime::spawn` (для async) или `tauri::async_runtime::spawn_blocking` (для CPU-bound). Tauri обёртывает Tokio и доступен из любого контекста.

## IPC-команды

Все команды в `lib.rs`, регистрируются через `invoke_handler![...]`. Параметры из JS автоматически конвертируются camelCase → snake_case для аргументов верхнего уровня. Поля внутри структур — как есть (без переименования).

## Capabilities

`src-tauri/capabilities/default.json` — список разрешений для frontend. Минимум нужен:
- `core:default`, `core:event:default`, `core:window:default`, `core:webview:default`, `core:path:default`
- `opener:default`, `dialog:default`
- `updater:default`, `process:default`

Sidecar-исполнение в Rust-коде (не из JS) не требует `shell:allow-execute`.

## Drag-and-drop

В `tauri.conf.json` окно должно иметь `"dragDropEnabled": true`. В React — `getCurrentWebview().onDragDropEvent(cb)`, payload содержит `.type: "over" | "drop" | "leave"` и `.paths`.

## File paths

Используй `app.path()`:
- `app_data_dir()` → `~/Library/Application Support/com.alexk.parrot/` — модели, settings.json
- `app_cache_dir()` → `~/Library/Caches/…` — tmp (очищается при старте)
- `app_log_dir()` → `~/Library/Logs/…` — tracing-appender daily rolling

## Events

Rust → JS: `app.emit("name", payload)`. JS → Rust: `invoke("cmd", { arg })`.

Payload должен быть `Serialize`. Для строгой сериализации имён полей — `#[serde(rename_all = "camelCase")]`.

## Single instance

Плагин `tauri-plugin-single-instance` — second launch фокусирует существующее окно вместо запуска нового.
