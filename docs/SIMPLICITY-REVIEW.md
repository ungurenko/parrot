# Аудит лишней сложности Parrot

Дата аудита: 2026-08-20  
Статус плана: 11 из 11 сделано

## Итог

Проект в целом не переусложнён. Главная лишняя сложность накопилась в CSS, повторяющемся frontend control flow и связности общего MLX-окружения. Сложность очереди, отмены процессов, локальных моделей и macOS-интеграции признана необходимой и в этот рефакторинг не входит.

## План упрощения

- [x] 1. Удалить доказанно мёртвые UI-файлы, asset, зависимости, IPC-команду, типы и экспорты. (2026-08-20)
- [x] 2. Сократить `Field` до трёх реально используемых компонентов. (2026-08-20)
- [x] 3. Удалить мёртвый CSS и объединить два поколения правил без изменения дизайна. (2026-08-20)
- [x] 4. Свести сохранение настроек к локальной `saveSettings` и общей ошибке. (2026-08-20)
- [x] 5. Сделать frontend-отмену задачи одной законченной операцией в `App`. (2026-08-20)
- [x] 6. Оставить updater с одним источником занятости. (2026-08-20)
- [x] 7. Упростить model progress: убрать фиктивный cache, объединить poller и expected size mapping, добавить общий frontend-тип стадии. (2026-08-20)
- [x] 8. Переиспользовать один ffmpeg-конвейер для локальных файлов и YouTube. (2026-08-20)
- [x] 9. Свести диспетчеризацию движка к одному простому Rust-helper. (2026-08-20)
- [x] 10. Убрать лишний mutex sender, обход к `AppHandle`, ручной cleanup token и UTC-алгоритм. (2026-08-20)
- [x] 11. Вынести общий Python/venv bootstrap в нейтральный `mlx_env.rs`. (2026-08-20)

## Не трогать

- Race-safe `CancelRegistry`, PID-регистрацию и `CancelRegistryGuard`.
- FIFO с prep-workers, `ActiveJobSlot` и область действия `PRELOAD_LOCK`.
- Inter-process registry summary-сервера, PID/start-time/command validation и SIGTERM → SIGKILL cleanup.
- Path canonicalization, ready markers, проверку размера моделей, Parakeet chunking и macOS Accessibility fallback.
- Release-цепочку sign → repack → re-sign → `latest.json`.
- Локальный Markdown-renderer, `userErrors.ts` и reducer событий задач.

## Проверка готовности

- `npx --yes knip --no-progress` — перечисленные мёртвые файлы, зависимости и runtime-экспорты исчезли. Остались 12 внутренних shadcn-экспортов и два release-скрипта, вызываемых динамически из `release_mac.mjs`; они сохранены намеренно.
- `npx --yes jscpd src src-tauri/src --min-lines 10 --min-tokens 50 --reporters console` — прежние клоны ffmpeg-нормализации и model progress исчезли; общее дублирование 0,71%.
- `npm run check` — 21 UI-тест и 59 Rust-тестов прошли, 2 ручных performance-теста штатно пропущены; frontend build, `cargo fmt --check` и строгий Clippy успешны.
- `npm run build:local` — собраны `Parrot.app` и `Parrot_0.4.22_aarch64.dmg`; app bundle проходит `codesign --verify --deep --strict` и запущен.
- Browser preview проверен в реальном Chromium при 1040×760 и 800×560: снимки исходного `HEAD` и новой версии побайтно совпадают в обоих размерах; все вкладки настроек открываются без ошибок консоли.
- Bundled ffmpeg создал тестовый 16 kHz mono PCM16 WAV. Указанный тестовый YouTube ID `BaW_jenozKc` стал недоступен; реальный download → normalize smoke успешно повторён с коротким публичным роликом `jNQXAC9IVRw`.

Итоговый исходный diff с новыми `mlx_env.rs` и этим отчётом: 627 добавлений и 1426 удалений. `src-tauri/binaries/`, пользовательские настройки и кэши моделей не менялись.
