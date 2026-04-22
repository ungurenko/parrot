import { useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import type { Job } from "../types";

interface Props {
  job: Job;
}

function renderMarkdown(md: string): React.ReactNode {
  const lines = md.split(/\r?\n/);
  const nodes: React.ReactNode[] = [];
  let listBuffer: string[] = [];
  let paragraphBuffer: string[] = [];

  const flushList = () => {
    if (listBuffer.length === 0) return;
    nodes.push(
      <ul key={`ul-${nodes.length}`} className="summary-list">
        {listBuffer.map((item, idx) => (
          <li key={idx}>{renderInline(item)}</li>
        ))}
      </ul>,
    );
    listBuffer = [];
  };

  const flushParagraph = () => {
    if (paragraphBuffer.length === 0) return;
    const text = paragraphBuffer.join(" ").trim();
    if (text) {
      nodes.push(
        <p key={`p-${nodes.length}`} className="summary-p">
          {renderInline(text)}
        </p>,
      );
    }
    paragraphBuffer = [];
  };

  for (const raw of lines) {
    const line = raw.trimEnd();
    if (line.startsWith("### ")) {
      flushList();
      flushParagraph();
      nodes.push(
        <h3 key={`h3-${nodes.length}`} className="summary-h3">
          {renderInline(line.slice(4))}
        </h3>,
      );
    } else if (line.startsWith("## ")) {
      flushList();
      flushParagraph();
      nodes.push(
        <h2 key={`h2-${nodes.length}`} className="summary-h2">
          {renderInline(line.slice(3))}
        </h2>,
      );
    } else if (line.startsWith("# ")) {
      flushList();
      flushParagraph();
      nodes.push(
        <h2 key={`h1-${nodes.length}`} className="summary-h2">
          {renderInline(line.slice(2))}
        </h2>,
      );
    } else if (/^\s*[-*]\s+/.test(line)) {
      flushParagraph();
      listBuffer.push(line.replace(/^\s*[-*]\s+/, ""));
    } else if (line.trim() === "") {
      flushList();
      flushParagraph();
    } else {
      flushList();
      paragraphBuffer.push(line);
    }
  }
  flushList();
  flushParagraph();
  return nodes;
}

function renderInline(text: string): React.ReactNode {
  const parts: React.ReactNode[] = [];
  const regex = /\*\*([^*]+)\*\*|\*([^*]+)\*|`([^`]+)`/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null;
  let keyIdx = 0;
  while ((match = regex.exec(text)) !== null) {
    if (match.index > lastIndex) {
      parts.push(text.slice(lastIndex, match.index));
    }
    if (match[1] !== undefined) {
      parts.push(<strong key={keyIdx++}>{match[1]}</strong>);
    } else if (match[2] !== undefined) {
      parts.push(<em key={keyIdx++}>{match[2]}</em>);
    } else if (match[3] !== undefined) {
      parts.push(
        <code key={keyIdx++} className="summary-code">
          {match[3]}
        </code>,
      );
    }
    lastIndex = match.index + match[0].length;
  }
  if (lastIndex < text.length) {
    parts.push(text.slice(lastIndex));
  }
  return parts.length > 0 ? parts : text;
}

const stageLabel = (stage?: string) => {
  switch (stage) {
    case "loading":
      return "Загружаю модель…";
    case "generating":
      return "Создаю конспект…";
    case "finalizing":
      return "Сохраняю…";
    default:
      return "Работаю…";
  }
};

export function SummaryPanel({ job }: Props) {
  const status = job.summaryStatus ?? "idle";
  const percent = job.summaryPercent ?? 0;
  const rendered = useMemo(
    () => (job.summary ? renderMarkdown(job.summary) : null),
    [job.summary],
  );

  const startSummary = async () => {
    if (!job.text || !job.outputPath) return;
    try {
      await invoke("summarize", {
        id: job.id,
        transcript: job.text,
        transcriptPath: job.outputPath,
      });
    } catch (e) {
      toast.error("Не удалось создать конспект", { description: String(e) });
    }
  };

  const cancelSummary = async () => {
    try {
      await invoke("cancel_summary", { id: job.id });
    } catch (e) {
      console.error("cancel_summary failed:", e);
    }
  };

  const copySummary = async () => {
    if (!job.summary) return;
    try {
      await navigator.clipboard.writeText(job.summary);
      toast.success("Конспект скопирован");
    } catch (e) {
      toast.error("Не удалось скопировать", { description: String(e) });
    }
  };

  const openInFinder = () => {
    if (!job.summaryPath) return;
    invoke("open_in_finder", { path: job.summaryPath }).catch((e) => {
      toast.error("Не удалось открыть в Finder", { description: String(e) });
    });
  };

  return (
    <div className="summary-card">
      <div className="summary-head">
        <div className="flex items-center gap-2">
          <span className="text-base">🪶</span>
          <h3 className="summary-title">Конспект</h3>
        </div>
        <div className="flex items-center gap-2">
          {status === "done" && (
            <>
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={copySummary}
              >
                ⧉ Копировать
              </Button>
              {job.summaryPath && (
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={openInFinder}
                >
                  ↗ Finder
                </Button>
              )}
              <Button
                type="button"
                variant="ghost"
                size="sm"
                onClick={startSummary}
                title="Перегенерировать"
              >
                ↻
              </Button>
            </>
          )}
          {status === "generating" && (
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={cancelSummary}
            >
              Отменить
            </Button>
          )}
        </div>
      </div>

      {status === "idle" && (
        <div className="summary-empty">
          <p className="summary-empty-text">
            Локальная модель соберёт из транскрипта краткое резюме, темы, тезисы
            и список действий.
          </p>
          <Button
            type="button"
            onClick={startSummary}
            disabled={!job.text || !job.outputPath}
          >
            🪶 Сгенерировать конспект
          </Button>
        </div>
      )}

      {status === "generating" && (
        <div className="summary-progress">
          <Progress value={Math.max(percent, 2)} />
          <div className="summary-progress-text">
            {stageLabel(job.summaryStage)} {percent}%
          </div>
        </div>
      )}

      {status === "error" && (
        <div className="flex flex-col gap-3">
          <Alert variant="destructive">
            <AlertTitle>Не удалось создать конспект</AlertTitle>
            <AlertDescription className="whitespace-pre-wrap break-words">
              {job.summaryError}
            </AlertDescription>
          </Alert>
          <Button type="button" variant="outline" onClick={startSummary}>
            Попробовать ещё раз
          </Button>
        </div>
      )}

      {status === "done" && rendered && (
        <article className="summary-body">{rendered}</article>
      )}
    </div>
  );
}
