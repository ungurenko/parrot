# Parrot

Локальное macOS-приложение для перевода аудио, видео и YouTube-ссылок в текст.

Разработано Александром Унгуренко в 2026 году.

## Скачать для Mac

[Скачать Parrot.dmg](https://github.com/ungurenko/parrot/releases/latest/download/Parrot.dmg)

Откройте скачанный файл и перетащите Parrot в папку “Программы”.

Подходит для Mac с Apple Silicon: M1, M2, M3, M4 и новее.

## Запуск

- `npm run tauri dev` — запуск приложения для разработки.
- `npm run build:local` — локальная сборка `.app` и `.dmg` без подписи обновлений.
- `npm run release:mac` — релизная сборка с подписью обновлений.
- `npm run check` — проверка интерфейса, Rust-тестов, форматирования и Clippy.

## Обновления

Parrot умеет проверять обновления через GitHub Releases. Подробности — в `docs/updates.md`.
