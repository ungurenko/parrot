export interface UserFacingError {
  title: string;
  message: string;
  action: string;
  technical?: string;
}

export function userErrorFrom(error: unknown): UserFacingError {
  const raw = String(error ?? "").trim();
  const normalized = raw.toLowerCase();

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

  if (normalized.includes("модель") && normalized.includes("не готов")) {
    return {
      title: "Модель ещё не готова",
      message: "Нужно один раз скачать и подготовить локальную модель.",
      action: "Откройте настройки модели и нажмите «Скачать».",
      technical: raw || undefined,
    };
  }

  if (normalized.includes("файл не найден") || normalized.includes("недоступен")) {
    return {
      title: "Файл недоступен",
      message: "Parrot не видит исходный файл или сохранённый транскрипт.",
      action: "Проверьте, что файл не удалён и папка доступна.",
      technical: raw || undefined,
    };
  }

  if (normalized.includes("отменено пользователем")) {
    return {
      title: "Действие отменено",
      message: "Parrot остановил задачу.",
      action: "Запустите запись заново, если текст всё ещё нужен.",
    };
  }

  if (normalized.includes("download") || normalized.includes("скачив")) {
    return {
      title: "Скачивание не завершилось",
      message: "Не получилось скачать нужные файлы модели или видео.",
      action: "Проверьте интернет и попробуйте ещё раз.",
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
