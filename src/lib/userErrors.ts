export interface UserFacingError {
  title: string;
  message: string;
  action: string;
  technical?: string;
}

function hasAny(text: string, needles: string[]): boolean {
  return needles.some((needle) => text.includes(needle));
}

export function userErrorFrom(error: unknown): UserFacingError {
  const raw = String(error ?? "").trim();
  const normalized = raw.toLowerCase();

  if (
    normalized === "cancelled" ||
    normalized === "canceled" ||
    normalized.includes("отменено пользователем")
  ) {
    return {
      title: "Действие отменено",
      message: "Parrot остановил задачу.",
      action: "Запустите запись заново, если текст всё ещё нужен.",
    };
  }

  if (
    hasAny(normalized, [
      "запись не найден",
      "не удалось прочитать транскрипт",
      "transcript",
    ])
  ) {
    return {
      title: "Запись недоступна",
      message: "Parrot не смог открыть запись из истории.",
      action: "Проверьте, что файл с текстом не удалён и папка доступна.",
      technical: raw || undefined,
    };
  }

  if (normalized.includes("пустой транскрипт")) {
    return {
      title: "В тексте нечего конспектировать",
      message: "Конспект можно сделать только по готовому тексту.",
      action: "Добавьте аудио или видео и сначала сделайте транскрипцию.",
      technical: raw || undefined,
    };
  }

  if (
    hasAny(normalized, [
      "sign in to confirm",
      "not a bot",
      "private video",
      "video unavailable",
      "this video is unavailable",
      "members-only",
      "yt-dlp failed",
      "youtube-загрузка",
      "downloaded audio not found",
    ])
  ) {
    return {
      title: "YouTube не отдал видео",
      message:
        "YouTube ограничил скачивание этого ролика или временно не отвечает.",
      action:
        "Откройте ссылку в браузере. Если видео доступно, скачайте видео вручную и добавьте файл в Parrot.",
      technical: raw || undefined,
    };
  }

  if (
    hasAny(normalized, [
      "qwen mlx",
      "mlx-qwen3-asr",
      "unknown qwen",
      "qwen server",
      "qwen warm",
    ])
  ) {
    return {
      title: "Режим качества недоступен",
      message: "Режим качества сейчас недоступен на этом Mac.",
      action: "Выберите быстрый режим или Whisper и запустите распознавание снова.",
      technical: raw || undefined,
    };
  }

  if (normalized.includes("неподдерживаемый формат")) {
    return {
      title: "Файл не подходит",
      message:
        "Parrot понимает аудио и видео: MP3, M4A, WAV, MP4, MOV и похожие форматы.",
      action: "Выберите другой файл или конвертируйте запись в MP3.",
    };
  }

  if (normalized.includes("youtube") || normalized.includes("url не похож")) {
    return {
      title: "Ссылка не похожа на YouTube",
      message: "Parrot умеет брать видео с обычных ссылок YouTube.",
      action: "Вставьте ссылку вида youtube.com/watch или youtu.be/...",
      technical: raw || undefined,
    };
  }

  if (
    hasAny(normalized, [
      "неизвестная модель",
      "неизвестный движок",
      "неизвестный язык",
      "некорректное сочетание",
      "пустая часть сочетания",
      "добавьте command",
      "добавьте option",
      "добавьте control",
      "добавьте shift",
    ])
  ) {
    return {
      title: "Настройку не удалось сохранить",
      message: "В настройке осталось старое или неподдерживаемое значение.",
      action: "Откройте настройки и выберите значение из списка.",
      technical: raw || undefined,
    };
  }

  if (
    (normalized.includes("модель") &&
      hasAny(normalized, ["не готов", "не скачана", "not ready"])) ||
    normalized.includes("model cache")
  ) {
    return {
      title: "Модель ещё не готова",
      message: "Нужно один раз скачать и подготовить локальную модель.",
      action: "Откройте настройки и нажмите кнопку скачивания рядом с нужной моделью.",
      technical: raw || undefined,
    };
  }

  if (normalized.includes("локальная модель уже занята")) {
    return {
      title: "Локальная модель занята",
      message: "Parrot уже создаёт конспект или перевод.",
      action: "Дождитесь завершения текущей задачи и повторите действие.",
      technical: raw || undefined,
    };
  }

  if (
    normalized.includes("модель перевода") ||
    normalized.includes("достигла лимита ответа")
  ) {
    return {
      title: "Перевод не создался",
      message: "Parrot не смог завершить локальный перевод.",
      action: "Подождите немного и попробуйте ещё раз. Если ошибка повторится, откройте логи в настройках.",
      technical: raw || undefined,
    };
  }

  if (
    normalized.includes("файл не найден") ||
    normalized.includes("не найден") ||
    normalized.includes("недоступен")
  ) {
    return {
      title: "Файл недоступен",
      message: "Parrot не видит исходный файл или сохранённый транскрипт.",
      action: "Проверьте, что файл не удалён и папка доступна.",
      technical: raw || undefined,
    };
  }

  if (
    hasAny(normalized, [
      "http get failed",
      "error sending request",
      "network",
      "connection",
      "timed out",
      "timeout",
      "download",
      "скачив",
      "huggingface",
      "ssl",
      "dns",
    ])
  ) {
    return {
      title: "Скачивание не завершилось",
      message: "Parrot не смог скачать модель, обновление или видео.",
      action: "Проверьте интернет, выключите VPN при необходимости и попробуйте ещё раз.",
      technical: raw || undefined,
    };
  }

  if (
    hasAny(normalized, [
      "pip install",
      "mlx_lm",
      "mlx-vlm",
      "mlx_vlm",
      "venv",
      "python",
      "server не успел",
      "server завершился",
      "модель конспекта",
      "конспект",
    ])
  ) {
    return {
      title: "Конспекты не подготовились",
      message: "Parrot не смог подготовить локальную модель для конспектов.",
      action:
        "Откройте Настройки → Конспект и нажмите «Установить окружение» ещё раз.",
      technical: raw || undefined,
    };
  }

  if (
    hasAny(normalized, [
      "ffmpeg",
      "invalid data",
      "conversion failed",
      "failed to load parakeet",
      "parakeet transcribe failed",
      "whisper",
      "transcribe failed",
      "raw ffmpeg failure",
    ])
  ) {
    return {
      title: "Аудио не удалось прочитать",
      message: "Parrot не смог извлечь звук из файла или видео.",
      action:
        "Попробуйте другой файл. Если это видео, сохраните его как MP3 или M4A и добавьте снова.",
      technical: raw || undefined,
    };
  }

  if (
    hasAny(normalized, [
      "accessibility",
      "ax error",
      "активное поле",
      "поле ввода",
      "системное событие клавиатуры",
      "вставлять текст автоматически",
      "универсальный доступ",
      "микрофон не найден",
      "неподдерживаемый формат микрофона",
      "запись слишком короткая",
    ])
  ) {
    return {
      title: "Диктовка не вставила текст",
      message:
        "Parrot не смог найти активное поле для текста или получить доступ к диктовке.",
      action:
        "Откройте Настройки → Диктовка и проверьте доступ macOS «Универсальный доступ». Если Parrot уже включён, выключите и включите его снова. Если ошибка останется, удалите Parrot из списка и добавьте заново.",
      technical: raw || undefined,
    };
  }

  if (
    hasAny(normalized, [
      "permission denied",
      "operation not permitted",
      "read-only",
      "папка сохранения",
      "путь сохранения",
      "failed to save",
      "не удалось сохранить",
    ])
  ) {
    return {
      title: "Не получилось сохранить файл",
      message: "Parrot не смог записать текст в выбранную папку.",
      action:
        "Выберите другую папку для готовых текстов или проверьте доступ к текущей папке.",
      technical: raw || undefined,
    };
  }

  return {
    title: "Что-то пошло не так",
    message: "Parrot не смог завершить действие.",
    action: "Попробуйте ещё раз. Если повторится, откройте логи в настройках.",
    technical: raw || undefined,
  };
}

export function formatErrorDescription(error: unknown): string {
  const userError = userErrorFrom(error);
  return `${userError.message}\n${userError.action}`;
}

// Backend commands reject user-initiated cancellations with the exact
// string "cancelled" (see ensure_not_cancelled in src-tauri). Surface
// them through one predicate so call sites never sniff raw strings.
export function isCancelledError(e: unknown): boolean {
  return String(e).includes("cancelled");
}
