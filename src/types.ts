type JobStatus = "queued" | "running" | "canceling" | "canceled" | "done" | "error";

export type JobStage = "preparing" | "extracting" | "downloading" | "transcribing" | null;

type SummaryStatus = "idle" | "generating" | "done" | "error";
export type SummaryStage = "loading" | "generating" | "finalizing";

export interface Job {
  id: string;
  sourceName: string;
  sourceKind?: "localFile" | "youtube";
  sourceValue?: string;
  status: JobStatus;
  stage: JobStage;
  percent: number;
  engine?: Engine;
  language?: TranscriptLanguage;
  text?: string;
  outputPath?: string;
  error?: string;
  summaryStatus?: SummaryStatus;
  summaryStage?: SummaryStage;
  summaryPercent?: number;
  summary?: string;
  summaryPath?: string;
  summaryError?: string;
}

export type Engine = "parakeet" | "whisper" | "qwen-0.6b" | "qwen-1.7b";
export type EngineMode = "fast" | "russian" | "hardAudio" | "manyLanguages";
export type Theme = "system" | "light" | "dark";

export const THEME_LABEL: Record<Theme, string> = {
  system: "Системная",
  light: "Светлая",
  dark: "Тёмная",
};

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

interface EngineStatus {
  available: boolean;
  modelReady: boolean;
  unavailableReason?: string | null;
}

export type EngineStatuses = Partial<Record<Engine, EngineStatus>>;

export interface ModelProgressDetail {
  percent: number;
  downloaded_bytes: number;
  total_bytes: number;
  speed_bytes_per_sec: number;
}

export type ModelStage = "installing" | "downloading" | "warmup" | "ready";

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
  summarizer_enabled: boolean;
  summary_model: SummaryModel;
  summarizer_promo_seen: boolean;
  dictation_enabled: boolean;
  dictation_hold_key: string;
  theme: Theme;
}

export interface SummarizerStatus {
  available: boolean;
  modelReady: boolean;
  unavailableReason?: string | null;
}

export type DictationPhase = "idle" | "recording" | "processing" | "error";

export interface DictationStatus {
  phase: DictationPhase;
}

export type SummaryModel = "qwen3-4b" | "gemma4-e2b";

export const SUMMARY_MODEL_LABEL: Record<SummaryModel, string> = {
  "qwen3-4b": "Qwen 3-4B Instruct",
  "gemma4-e2b": "Gemma 4 E2B-it",
};

export const SUMMARY_MODEL_SIZE: Record<SummaryModel, string> = {
  "qwen3-4b": "~2.3 ГБ",
  "gemma4-e2b": "~3.6 ГБ",
};

export const SUMMARY_MODEL_BADGE: Record<SummaryModel, string> = {
  "qwen3-4b": "стабильная",
  "gemma4-e2b": "новая",
};

export const DEFAULT_SUMMARY_MODEL: SummaryModel = "qwen3-4b";

export const CANCELLED_MARKER = "Отменено пользователем";

export const ACTIVE_JOB_SWITCH_HINT =
  "Дождитесь окончания транскрибации, чтобы сменить модель.";
export const ACTIVE_JOB_DELETE_HINT =
  "Дождитесь окончания транскрибации, чтобы удалить модель.";

export interface HistoryEntry {
  id: string;
  sourceName: string;
  sourceKind?: "localFile" | "youtube";
  sourceValue?: string;
  engine: string;
  language: string;
  createdAt: string;
  outputPath: string;
  summaryPath?: string;
}

export interface LoadedHistoryEntry {
  entry: HistoryEntry;
  text: string;
  summary?: string;
}
