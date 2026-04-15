export type JobStatus = "queued" | "running" | "done" | "error";

export type JobStage = "preparing" | "extracting" | "downloading" | "transcribing" | null;

export interface Job {
  id: string;
  sourceName: string;
  status: JobStatus;
  stage: JobStage;
  percent: number;
  text?: string;
  outputPath?: string;
  error?: string;
}

export type Engine = "parakeet" | "whisper" | "qwen-0.6b" | "qwen-1.7b";

export const ENGINE_LABEL: Record<Engine, string> = {
  parakeet: "Parakeet V3",
  whisper: "Whisper Large-v3 turbo",
  "qwen-0.6b": "Qwen3-ASR 0.6B MLX",
  "qwen-1.7b": "Qwen3-ASR 1.7B MLX",
};

export const ENGINE_SIZE: Record<Engine, string> = {
  parakeet: "~1.3 ГБ",
  whisper: "~1.2 ГБ",
  "qwen-0.6b": "~1.2 ГБ",
  "qwen-1.7b": "~3.4 ГБ",
};

export const isQwenEngine = (engine: Engine) => engine.startsWith("qwen-");

export type ModelStatuses = Partial<Record<Engine, boolean>>;

export interface ModelProgressDetail {
  percent: number;
  downloaded_bytes: number;
  total_bytes: number;
  speed_bytes_per_sec: number;
}

export type TranscriptLanguage =
  | "auto"
  | "ru"
  | "en"
  | "de"
  | "fr"
  | "es"
  | "it"
  | "pt"
  | "uk";

export const LANGUAGE_LABEL: Record<TranscriptLanguage, string> = {
  auto: "Авто",
  ru: "Русский",
  en: "Английский",
  de: "Немецкий",
  fr: "Французский",
  es: "Испанский",
  it: "Итальянский",
  pt: "Португальский",
  uk: "Украинский",
};

export interface Settings {
  save_dir: string;
  onboarded: boolean;
  engine: Engine;
  language: TranscriptLanguage;
}
