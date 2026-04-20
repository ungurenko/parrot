import { useMemo } from "react";
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
import { ENGINE_LABEL, type Job } from "../types";

interface Props {
  job: Job;
  onReset: () => void;
  engineLabel?: string;
}

function formatFileName(name: string): string {
  return name.length > 48 ? `${name.slice(0, 45)}…` : name;
}

export function ResultView({ job, onReset, engineLabel }: Props) {
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
        <span className="pill">
          <span className="led" />
          {formatFileName(job.sourceName)}
        </span>
      </div>

      <div className="doc-card min-h-0 flex-1">
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

        <div className="segments">
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
      </div>
    </div>
  );
}
