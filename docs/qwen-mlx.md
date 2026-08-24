# Qwen3-ASR MLX

> Для моделей конспекта см. `docs/local-summary-models.md`.

Qwen доступен как дополнительный локальный движок на Apple Silicon с macOS 14+. Parakeet остаётся основным и выбран по умолчанию.

## Подготовка

Откройте **Настройки → Модели** и нажмите «Скачать и выбрать». Parrot сам скачает standalone Python, установит Qwen runtime и подготовит выбранную модель. Системный Python, Homebrew и команды в Terminal не нужны.

Окружение хранится в `~/Library/Application Support/com.alexk.parrot/.qwen-mlx/venv`, модели — в соседнем каталоге `models/qwen-mlx`. На macOS 11–13 используйте Parakeet или Whisper: Apple MLX на этих системах не поддерживается.

## Быстрый тест

Для короткого локального файла:

```bash
tools/qwen_mlx_probe.py "/path/to/audio.mp3" --model 0.6b
```

Для YouTube-ссылки:

```bash
tools/qwen_mlx_probe.py "https://www.youtube.com/watch?v=..." --model 0.6b
```

Результаты сохраняются в `qwen-results`: отдельно текст и файл с временем/памятью.

## Проверка 1.7B

Сначала лучше проверить 0.6B. Если все нормально:

```bash
tools/qwen_mlx_probe.py "/path/to/audio.mp3" --model 1.7b
```

## Локальный сервер

Для отдельной проверки сервера:

```bash
tools/qwen_mlx_server.sh Qwen/Qwen3-ASR-0.6B
```

По умолчанию сервер запускается на `http://127.0.0.1:8765`.

## В приложении

В настройках доступны два варианта:

- Qwen3-ASR 0.6B MLX
- Qwen3-ASR 1.7B MLX

Первое нажатие «Скачать и выбрать» установит runtime, скачает модель, проверит запуск и только после успеха сделает её активной. Если сеть прервётся, повторное нажатие продолжит подготовку.

## Для разработки

`tools/setup_qwen_mlx.sh` остаётся запасным dev-сценарием для repo-local окружения. Пользователям DMG он не нужен.
