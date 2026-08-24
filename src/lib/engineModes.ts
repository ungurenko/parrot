import {
  ENGINE_LABEL,
  ENGINE_SIZE,
  type Engine,
  type EngineMode,
} from "../types";

export interface EngineModeOption {
  id: EngineMode;
  engine: Engine;
  title: string;
  subtitle: string;
  detail: string;
  technicalName: string;
  size: string;
  badge?: string;
  primary?: boolean;
}

export const ENGINE_MODES: EngineModeOption[] = [
  {
    id: "fast",
    engine: "parakeet",
    title: "Быстро",
    subtitle: "Для встреч, лекций и обычных записей",
    detail: "Самый спокойный старт: быстро работает на Mac и подходит для русского.",
    technicalName: ENGINE_LABEL.parakeet,
    size: ENGINE_SIZE.parakeet,
    badge: "рекомендуется",
    primary: true,
  },
  {
    id: "manyLanguages",
    engine: "whisper",
    title: "Много языков",
    subtitle: "Для английского, европейских и редких языков",
    detail: "Медленнее, зато уверенно работает с большим числом языков.",
    technicalName: ENGINE_LABEL.whisper,
    size: ENGINE_SIZE.whisper,
  },
  {
    id: "russian",
    engine: "qwen-0.6b",
    title: "Лучше для русского",
    subtitle: "Когда важнее качество текста",
    detail: "Хороший вариант для русской речи, если запись важная.",
    technicalName: ENGINE_LABEL["qwen-0.6b"],
    size: ENGINE_SIZE["qwen-0.6b"],
  },
  {
    id: "hardAudio",
    engine: "qwen-1.7b",
    title: "Сложная запись",
    subtitle: "Для шума, акцентов и тяжёлого аудио",
    detail: "Самый тяжёлый режим. Для длинных файлов лучше Mac с 32+ ГБ памяти.",
    technicalName: ENGINE_LABEL["qwen-1.7b"],
    size: ENGINE_SIZE["qwen-1.7b"],
    badge: "качество",
  },
];

export function modeOptionForEngine(engine: Engine): EngineModeOption {
  return ENGINE_MODES.find((option) => option.engine === engine) ?? ENGINE_MODES[0];
}

export function readableEngineName(engine: Engine): string {
  const option = modeOptionForEngine(engine);
  return `${option.title} (${option.technicalName})`;
}
