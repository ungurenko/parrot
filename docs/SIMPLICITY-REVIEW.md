# Аудит лишней сложности Parrot

Дата повторного аудита: 2026-08-27
Статус второго прохода: 4 из 4 сделано

## Итог второго прохода

После основного рефакторинга остались только три лишних frontend-экспорта и один короткий повтор сбора MLX-путей. Экспорты убраны, Parakeet теперь переиспользует общий `mlx_env::candidate_roots()`, а прежний приоритет Application Support зафиксирован отдельным тестом.

## План второго прохода

- [x] 1. Сделать `buttonVariants` внутренней деталью `Button`. (2026-08-27)
- [x] 2. Сделать `ScrollBar` внутренней деталью `ScrollArea`. (2026-08-27)
- [x] 3. Сделать `THEME_STORAGE_KEY` внутренней константой `useTheme`. (2026-08-27)
- [x] 4. Переиспользовать общий список dev-путей MLX в Parakeet, сохранив приоритет Application Support и добавив фиксирующий тест. (2026-08-27)

## Проверка второго прохода

- Фиксирующий Rust-тест порядка MLX-путей прошёл до и после упрощения.
- `npx --yes knip --no-progress --reporter compact` — лишние экспорты исчезли. Остались только три release-скрипта, которые вызываются динамически из `release_mac.mjs` и `prepare_release.mjs`.
- `npx --yes jscpd src src-tauri/src --min-lines 10 --min-tokens 50 --reporters console` — 0 клонов, 0,00% дублирования на выбранном пороге.
- `npm run check` — 31 frontend-тест и 73 Rust-теста прошли, 3 ручных performance-теста штатно пропущены; TypeScript/Vite build, `cargo fmt --check` и строгий Clippy успешны.
- `npm run build:local` — собраны `Parrot.app` и `Parrot_0.4.26_aarch64.dmg`; `codesign --verify --deep --strict` успешен, собранное приложение запущено без системных ошибок и корректно закрыто.
- Исходный diff второго прохода: 4 файла, 22 добавления и 18 удалений. Рабочий код сокращён на 10 строк; 14 строк добавлены как тест-страховка. Файлы, зависимости и продуктовые состояния или ветвления не добавлялись и не удалялись.

Commit, push и выпуск новой версии не выполнялись. `src-tauri/binaries/`, модели, очередь, отмена процессов и release-цепочка не менялись.

## Прошлый аудит (архив)

### Аудит от 2026-08-20

Статус плана: 11 из 11 сделано

### Итог

Проект в целом не переусложнён. Главная лишняя сложность накопилась в CSS, повторяющемся frontend control flow и связности общего MLX-окружения. Сложность очереди, отмены процессов, локальных моделей и macOS-интеграции признана необходимой и в этот рефакторинг не входит.

### План упрощения

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

### Не трогать

- Race-safe `CancelRegistry`, PID-регистрацию и `CancelRegistryGuard`.
- FIFO с prep-workers, `ActiveJobSlot` и область действия `PRELOAD_LOCK`.
- Inter-process registry summary-сервера, PID/start-time/command validation и SIGTERM → SIGKILL cleanup.
- Path canonicalization, ready markers, проверку размера моделей, Parakeet chunking и macOS Accessibility fallback.
- Release-цепочку sign → repack → re-sign → `latest.json`.
- Локальный Markdown-renderer, `userErrors.ts` и reducer событий задач.

### Проверка готовности

- `npx --yes knip --no-progress` — перечисленные мёртвые файлы, зависимости и runtime-экспорты исчезли. Остались 12 внутренних shadcn-экспортов и два release-скрипта, вызываемых динамически из `release_mac.mjs`; они сохранены намеренно.
- `npx --yes jscpd src src-tauri/src --min-lines 10 --min-tokens 50 --reporters console` — прежние клоны ffmpeg-нормализации и model progress исчезли; общее дублирование 0,71%.
- `npm run check` — 21 UI-тест и 59 Rust-тестов прошли, 2 ручных performance-теста штатно пропущены; frontend build, `cargo fmt --check` и строгий Clippy успешны.
- `npm run build:local` — собраны `Parrot.app` и `Parrot_0.4.22_aarch64.dmg`; app bundle проходит `codesign --verify --deep --strict` и запущен.
- Browser preview проверен в реальном Chromium при 1040×760 и 800×560: снимки исходного `HEAD` и новой версии побайтно совпадают в обоих размерах; все вкладки настроек открываются без ошибок консоли.
- Bundled ffmpeg создал тестовый 16 kHz mono PCM16 WAV. Указанный тестовый YouTube ID `BaW_jenozKc` стал недоступен; реальный download → normalize smoke успешно повторён с коротким публичным роликом `jNQXAC9IVRw`.

Итоговый исходный diff с новыми `mlx_env.rs` и этим отчётом: 627 добавлений и 1426 удалений. `src-tauri/binaries/`, пользовательские настройки и кэши моделей не менялись.
