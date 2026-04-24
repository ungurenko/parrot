import { useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  ENGINE_LABEL,
  SUMMARIZER_MODEL_SIZE,
  type Job,
  type Settings,
} from "../types";
import { SummaryPanel } from "./SummaryPanel";

interface Props {
  job: Job;
  onReset: () => void;
  engineLabel?: string;
  settings: Settings;
  onSettingsChange: (next: Settings) => void;
}

export function ResultView({
  job,
  onReset,
  engineLabel,
  settings,
  onSettingsChange,
}: Props) {
  const [transcriptExpanded, setTranscriptExpanded] = useState(false);
  const [pendingAutoDownload, setPendingAutoDownload] = useState(false);
  const [promoBusy, setPromoBusy] = useState(false);
  const summarizerEnabled = settings.summarizer_enabled;
  const showPromoBanner =
    !settings.summarizer_enabled && !settings.summarizer_promo_seen;

  const segments = useMemo(() => {
    if (!job.text) return [] as string[];
    const trimmed = job.text.trim();
    const split = trimmed.split(/\n{2,}/g).map((s) => s.trim()).filter(Boolean);
    if (split.length > 1) return split;
    return trimmed
      .split(/\n/g)
      .map((s) => s.trim())
      .filter(Boolean);
  }, [job.text]);

  const updateSettings = async (patch: Partial<Settings>) => {
    const next = { ...settings, ...patch };
    try {
      await invoke("set_settings", { new: next });
      onSettingsChange(next);
      return true;
    } catch (e) {
      toast.error("Не удалось сохранить настройку", { description: String(e) });
      return false;
    }
  };

  const enableSummarizerAndDownload = async () => {
    if (promoBusy) return;
    setPromoBusy(true);
    const ok = await updateSettings({
      summarizer_enabled: true,
      summarizer_promo_seen: true,
    });
    if (ok) setPendingAutoDownload(true);
    setPromoBusy(false);
  };

  const dismissPromo = () => {
    void updateSettings({ summarizer_promo_seen: true });
  };

  if (job.status === "error") {
    return (
      <div className="flex flex-col gap-3">
        <div className="flex items-center justify-between">
          <button type="button" className="ghost-btn" onClick={onReset}>
            <svg
              width="12"
              height="12"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth={2.5}
              strokeLinecap="round"
            >
              <path d="M12 5v14M5 12h14" />
            </svg>
            Новая транскрипция
          </button>
        </div>
        <Alert variant="destructive" className="glass-modal">
          <AlertTitle>Ошибка</AlertTitle>
          <AlertDescription className="whitespace-pre-wrap">
            {job.error}
          </AlertDescription>
        </Alert>
      </div>
    );
  }

  if (job.status === "canceled") {
    return (
      <div className="flex flex-col gap-3">
        <button type="button" className="ghost-btn self-start" onClick={onReset}>
          <svg
            width="12"
            height="12"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth={2.5}
            strokeLinecap="round"
          >
            <path d="M12 5v14M5 12h14" />
          </svg>
          Новая транскрипция
        </button>
        <Empty className="glass-surface border border-white/70">
          <EmptyHeader>
            <EmptyMedia className="text-2xl">🚫</EmptyMedia>
            <EmptyTitle>Задача отменена</EmptyTitle>
            <EmptyDescription>
              Можно добавить файл заново, если расшифровка всё ещё нужна.
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      </div>
    );
  }

  if (job.status !== "done" || job.text === undefined) {
    return (
      <Empty className="glass-surface border border-white/70">
        <EmptyHeader>
          <EmptyMedia className="text-2xl">⏳</EmptyMedia>
          <EmptyTitle>Задача ещё не завершена</EmptyTitle>
          <EmptyDescription>
            Текст появится здесь, когда расшифровка будет готова.
          </EmptyDescription>
        </EmptyHeader>
      </Empty>
    );
  }

  const copyText = async () => {
    try {
      await navigator.clipboard.writeText(job.text ?? "");
      toast.success("Скопировано");
    } catch (e) {
      toast.error("Не удалось скопировать текст", {
        description: String(e),
      });
    }
  };

  const revealInFinder = () => {
    if (!job.outputPath) return;
    invoke("open_in_finder", { path: job.outputPath }).catch((e) => {
      toast.error("Не удалось открыть в Finder", { description: String(e) });
    });
  };

  const wordCount = (job.text ?? "").trim().split(/\s+/).filter(Boolean).length;
  const charCount = (job.text ?? "").length;
  const engine = engineLabel ?? ENGINE_LABEL.parakeet;

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      <div className="flex items-center justify-between gap-3 flex-wrap">
        <button type="button" className="ghost-btn" onClick={onReset}>
          <svg
            width="12"
            height="12"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth={2.5}
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <path d="M12 5v14M5 12h14" />
          </svg>
          Новая транскрипция
        </button>
        <span className="pill min-w-0 flex-1 max-w-[60%] justify-end">
          <span className="led" />
          <span title={job.sourceName}>{job.sourceName}</span>
        </span>
      </div>

      {showPromoBanner && (
        <div className="summary-banner" role="region" aria-label="Конспект">
          <div className="summary-banner-icon" aria-hidden="true">
            🪶
          </div>
          <div className="summary-banner-body">
            <div className="summary-banner-title">
              Хотите конспект из этой записи?
            </div>
            <div className="summary-banner-text">
              Локальная модель Qwen 3-4B ({SUMMARIZER_MODEL_SIZE}, оффлайн)
              соберёт краткое резюме, темы, тезисы и список действий.
            </div>
          </div>
          <div className="summary-banner-actions">
            <Button
              type="button"
              size="sm"
              onClick={enableSummarizerAndDownload}
              disabled={promoBusy}
            >
              ⬇︎ Скачать модель
            </Button>
            <button
              type="button"
              className="summary-banner-close"
              onClick={dismissPromo}
              aria-label="Скрыть подсказку"
              title="Скрыть"
            >
              ×
            </button>
          </div>
        </div>
      )}

      <div className="result-scroll min-h-0 flex-1">
        <div className="doc-card">
          <div className="doc-head">
            <div className="min-w-0">
              <h2 title={job.sourceName}>{job.sourceName}</h2>
              <div className="meta-row">
                <span>
                  <b>{wordCount}</b> слов
                </span>
                <span>
                  <b>{charCount}</b> символов
                </span>
                <span>
                  <b>{engine}</b>
                </span>
              </div>
            </div>
            <div className="export-group">
              <button
                type="button"
                className="export-btn txt"
                onClick={revealInFinder}
                disabled={!job.outputPath}
                title="Открыть .txt в Finder"
              >
                <span className="dot" />
                TXT
              </button>
              <button
                type="button"
                className="export-btn srt"
                disabled
                title="Скоро"
              >
                <span className="dot" />
                SRT
              </button>
              <button
                type="button"
                className="export-btn md"
                disabled
                title="Скоро"
              >
                <span className="dot" />
                MD
              </button>
              <button type="button" className="export-btn" onClick={copyText}>
                ⧉ Copy
              </button>
              <button
                type="button"
                className="export-btn"
                onClick={revealInFinder}
                disabled={!job.outputPath}
              >
                ↗ Finder
              </button>
            </div>
          </div>

          <div
            className={`segments ${
              summarizerEnabled && !transcriptExpanded ? "segments-collapsed" : ""
            }`}
          >
            {segments.length > 0 ? (
              segments.map((seg, i) => (
                <div key={i} className="seg">
                  {seg}
                </div>
              ))
            ) : (
              <div className="seg" style={{ color: "var(--ink-3)" }}>
                Текст пустой.
              </div>
            )}
          </div>

          {summarizerEnabled && segments.length > 0 && (
            <button
              type="button"
              className="transcript-toggle"
              onClick={() => setTranscriptExpanded((v) => !v)}
            >
              {transcriptExpanded
                ? "Свернуть транскрипт ▲"
                : "Развернуть транскрипт ▼"}
            </button>
          )}
        </div>

        {summarizerEnabled && (
          <SummaryPanel
            job={job}
            autoStartDownload={pendingAutoDownload}
          />
        )}
      </div>
    </div>
  );
}
