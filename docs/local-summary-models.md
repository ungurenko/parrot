# Локальные модели конспекта

Parrot создаёт конспекты локально. Транскрипт остаётся на компьютере и не отправляется в облако.

## Модели

| Модель | ID в настройках | HF repo | Размер | Роль |
| --- | --- | --- | --- | --- |
| Qwen 3-4B Instruct | `qwen3-4b` | `mlx-community/Qwen3-4B-Instruct-2507-4bit` | ~2.3 ГБ | стабильная модель |
| Gemma 4 E2B-it | `gemma4-e2b` | `mlx-community/gemma-4-e2b-it-4bit` | ~3.6 ГБ | новая модель для сравнения русского текста |

## Окружение

Приложение само ставит локальный Python и MLX-пакеты в Application Support. Для разработки можно подготовить окружение командой:

```bash
tools/setup_qwen_mlx.sh
```

Для Gemma важны совместимые версии MLX: `mlx==0.31.1`, `mlx-lm==0.31.2`, `mlx-vlm==0.4.3`. Более свежая связка `mlx-lm==0.31.3` ломает загрузку Gemma 4.

## Проверка

```bash
printf 'Короткий русский текст для проверки конспекта.' > /tmp/parrot-summary-probe.txt
tools/probe_summary_model.py --model qwen3-4b --transcript /tmp/parrot-summary-probe.txt
tools/probe_summary_model.py --model gemma4-e2b --transcript /tmp/parrot-summary-probe.txt
```

Обе команды должны вернуть `"ok": true`.

## Правило выбора по умолчанию

Qwen остаётся дефолтом до живого сравнения на реальных русских расшифровках.
Gemma можно сделать дефолтом после проверки скорости, памяти и качества конспекта на MacBook M2 16 GB.
