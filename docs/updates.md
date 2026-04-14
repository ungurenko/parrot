# Обновления Parrot

Parrot обновляется по схеме, похожей на Handy: приложение проверяет файл `latest.json` в GitHub Releases, скачивает подписанный архив обновления и перезапускается.

## Что уже настроено

- В приложении включен Tauri Updater.
- Готовая сборка создает файлы обновления и подписи.
- В настройках приложения есть кнопка проверки обновлений.
- Публичный ключ обновлений хранится в `src-tauri/tauri.conf.json`.

## Что нужно для публикации

1. Создать GitHub-репозиторий `ungurenko/parrot` или заменить адрес в `src-tauri/tauri.conf.json` на фактический репозиторий.
2. Хранить приватный ключ обновлений только локально или в секретах GitHub Actions.
3. Перед сборкой передать приватный ключ и пустой пароль:

```bash
export TAURI_SIGNING_PRIVATE_KEY="$(cat "$HOME/.parrot-updater.key")"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
```

4. Увеличить версию в `package.json`, `src-tauri/Cargo.toml` и `src-tauri/tauri.conf.json`.
5. Собрать приложение:

```bash
npm run tauri build
```

6. Создать `latest.json`:

```bash
npm run release:latest-json
```

7. Загрузить в GitHub Releases:
   - `src-tauri/target/release/bundle/dmg/Parrot_0.1.0_aarch64.dmg`
   - `src-tauri/target/release/bundle/macos/Parrot.app.tar.gz`
   - `src-tauri/target/release/bundle/macos/Parrot.app.tar.gz.sig`
   - `src-tauri/target/release/bundle/macos/latest.json`

Важно: если потерять приватный ключ, старые версии приложения больше не смогут принимать новые обновления.
