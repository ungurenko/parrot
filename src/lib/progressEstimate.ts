import type { JobStage, ModelProgressDetail } from "../types";

type ModelStage = "downloading" | "warmup" | "ready";

export interface ProgressMessage {
  title: string;
  detail: string;
}

export function processingProgressMessage({
  stage,
  percent,
  elapsedMs,
}: {
  stage: JobStage;
  percent: number;
  elapsedMs?: number;
}): ProgressMessage {
  const title = titleForStage(stage, percent);
  const remaining = estimateRemaining(percent, elapsedMs);
  return {
    title,
    detail: remaining
      ? `Осталось примерно ${remaining}.`
      : "Обычно это занимает несколько минут.",
  };
}

export function modelProgressMessage({
  stage,
  percent,
  speedBytesPerSec,
  detail,
}: {
  stage: ModelStage;
  percent: number;
  speedBytesPerSec?: number;
  detail?: ModelProgressDetail | null;
}): ProgressMessage {
  if (stage === "ready" || percent >= 100) {
    return {
      title: "Модель готова",
      detail: "Можно начинать работу.",
    };
  }
  if (stage === "warmup") {
    return {
      title: `Подготовка почти закончена · ${percent}%`,
      detail: "Модель загружается в память. Обычно это 10-30 секунд.",
    };
  }

  const speed = speedBytesPerSec ?? detail?.speed_bytes_per_sec ?? 0;
  return {
    title: `Скачиваю модель · ${percent}%`,
    detail:
      speed > 0 && speed < 120_000
        ? "Сеть медленная, но загрузка продолжается."
        : "Файлы скачиваются один раз. После этого всё работает локально.",
  };
}

function titleForStage(stage: JobStage, percent: number): string {
  switch (stage) {
    case "preparing":
      return "Готовлю ссылку";
    case "downloading":
      return percent > 0 ? `Скачиваю · ${percent}%` : "Скачиваю";
    case "extracting":
      return percent > 0 ? `Достаю аудио · ${percent}%` : "Достаю аудио";
    case "transcribing":
      return percent > 0 ? `Распознаю речь · ${percent}%` : "Распознаю речь";
    default:
      return "Работаю";
  }
}

function estimateRemaining(percent: number, elapsedMs?: number): string | null {
  if (!elapsedMs || percent <= 5 || percent >= 95) return null;
  const remainingMs = (elapsedMs * (100 - percent)) / percent;
  const minutes = Math.max(1, Math.round(remainingMs / 60_000));
  if (minutes < 60) return `${minutes} мин`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest > 0 ? `${hours} ч ${rest} мин` : `${hours} ч`;
}
