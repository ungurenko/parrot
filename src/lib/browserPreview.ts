import type { Job, Theme } from "../types";

type PreviewStage = Exclude<Job["stage"], null>;
type PreviewTheme = Exclude<Theme, "system">;

export interface BrowserPreview {
  jobs: Job[];
  processing: boolean;
  theme?: PreviewTheme;
}

function parseStage(value: string | null): PreviewStage {
  switch (value) {
    case "preparing":
    case "extracting":
    case "downloading":
    case "transcribing":
      return value;
    default:
      return "transcribing";
  }
}

function parseTheme(value: string | null): PreviewTheme | undefined {
  return value === "light" || value === "dark" ? value : undefined;
}

function parsePercent(value: string | null): number {
  const percent = Number(value ?? "42");
  return Number.isFinite(percent)
    ? Math.min(100, Math.max(0, Math.round(percent)))
    : 42;
}

export function createBrowserPreview(
  search: string,
  enabled: boolean,
): BrowserPreview {
  if (!enabled) return { jobs: [], processing: false };

  const params = new URLSearchParams(search);
  const theme = parseTheme(params.get("theme"));
  if (params.get("preview") !== "processing") {
    return { jobs: [], processing: false, theme };
  }

  const requestedStatus = params.get("status");
  const status: Job["status"] =
    requestedStatus === "queued" || requestedStatus === "canceling"
      ? requestedStatus
      : "running";
  const sourceKind = params.get("source") === "file" ? "localFile" : "youtube";
  const main: Job = {
    id: "preview-processing",
    sourceName:
      params.get("name") ??
      "Как устроена локальная транскрибация на Mac — подробный разговор",
    sourceKind,
    status,
    stage: status === "queued" ? null : parseStage(params.get("stage")),
    percent: status === "queued" ? 0 : parsePercent(params.get("percent")),
    engine: "parakeet",
    language: "ru",
  };

  const queued: Job = {
    id: "preview-queued",
    sourceName: "Следующая встреча.m4a",
    sourceKind: "localFile",
    status: "queued",
    stage: null,
    percent: 0,
    engine: "qwen-0.6b",
    language: "ru",
  };
  const jobs = params.get("queue") === "1" ? [main, queued] : [main];

  return { jobs, processing: true, theme };
}
