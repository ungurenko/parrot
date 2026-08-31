import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import {
  cleanupTauriListeners,
  isTauriRuntime,
  listenInTauri,
} from "@/lib/runtime";
import { formatErrorDescription, isCancelledError, userErrorFrom } from "@/lib/userErrors";
import { summaryResult } from "@/lib/jobArtifacts";
import {
  DEFAULT_SUMMARY_MODEL,
  SUMMARY_MODEL_SIZE,
  type Job,
  type ModelStage,
  type Settings,
  type SummarizerStatus,
} from "../types";

interface Props {
  job: Job;
  settings: Settings;
  /** При первом рендере панель сама запросит модель к скачиванию. */
  autoStartDownload?: boolean;
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

export function SummaryPanel({ job, settings, autoStartDownload }: Props) {
  const status = job.summary?.status ?? "idle";
  const percent = job.summary?.status === "generating" ? job.summary.percent : 0;
  const summary = summaryResult(job.summary);
  const summaryModel = settings.summary_model ?? DEFAULT_SUMMARY_MODEL;
  const summaryModelSize = SUMMARY_MODEL_SIZE[summaryModel];
  const rendered = useMemo(
    () => (summary ? renderMarkdown(summary.content) : null),
    [summary],
  );

  const [summarizerStatus, setSummarizerStatus] =
    useState<SummarizerStatus | null>(null);
  const [modelInstalling, setModelInstalling] = useState(false);
  const [modelProgress, setModelProgress] = useState(0);
  const [modelStage, setModelStage] = useState<ModelStage>("downloading");
  const [modelError, setModelError] = useState<string | null>(null);

  const refreshSummarizerStatus = async () => {
    if (!isTauriRuntime()) {
      const previewStatus = { available: true, modelReady: true };
      setSummarizerStatus(previewStatus);
      return previewStatus;
    }

    try {
      const s = await invoke<SummarizerStatus>("get_summarizer_status");
      setSummarizerStatus(s);
      return s;
    } catch (e) {
      console.error("get_summarizer_status failed:", e);
      return null;
    }
  };

  useEffect(() => {
    refreshSummarizerStatus();
  }, []);

  useEffect(() => {
    const listeners = [
      listenInTauri<number>("summary_model:progress", (e) => {
        setModelProgress(e.payload);
      }),
      listenInTauri<ModelStage>(
        "summary_model:stage",
        (e) => setModelStage(e.payload),
      ),
    ];
    return () => {
      cleanupTauriListeners(listeners);
    };
  }, []);

  const installModel = async () => {
    if (modelInstalling) return;
    setModelInstalling(true);
    setModelError(null);
    setModelProgress(1);
    setModelStage("downloading");
    if (!isTauriRuntime()) {
      setModelProgress(100);
      setSummarizerStatus({ available: true, modelReady: true });
      setModelInstalling(false);
      return;
    }

    try {
      await invoke("download_summarizer_model");
      setModelProgress(100);
      await refreshSummarizerStatus();
    } catch (e: unknown) {
      if (isCancelledError(e)) {
        setModelProgress(0);
        setModelStage("downloading");
      } else {
        setModelError(formatErrorDescription(e));
      }
    } finally {
      setModelInstalling(false);
    }
  };

  // Авто-старт скачивания: пользователь нажал «Скачать модель» в промо-баннере
  // ResultView, и панель только что появилась. Запускаем один раз.
  const [autoStartHandled, setAutoStartHandled] = useState(false);
  useEffect(() => {
    if (autoStartHandled) return;
    if (!autoStartDownload) return;
    if (summarizerStatus === null) return; // ждём первого рефреша
    setAutoStartHandled(true);
    if (summarizerStatus.available && !summarizerStatus.modelReady) {
      installModel();
    }
  }, [autoStartDownload, summarizerStatus, autoStartHandled]);

  const startSummary = async () => {
    if (!job.text || !job.outputPath) return;
    if (!isTauriRuntime()) return;
    try {
      await invoke("summarize", {
        id: job.id,
        transcript: job.text,
        transcriptPath: job.outputPath,
      });
    } catch (e) {
      const friendly = userErrorFrom(e);
      toast.error(friendly.title, { description: formatErrorDescription(e) });
    }
  };

  const cancelSummary = async () => {
    if (!isTauriRuntime()) return;
    try {
      const canceled = await invoke<boolean>("cancel_summary", { id: job.id });
      if (!canceled) {
        toast.info("Конспект уже сохраняется");
      }
    } catch (e) {
      console.error("cancel_summary failed:", e);
    }
  };

  const copySummary = async () => {
    if (!summary) return;
    try {
      await navigator.clipboard.writeText(summary.content);
      toast.success("Конспект скопирован");
    } catch (e) {
      const friendly = userErrorFrom(e);
      toast.error(friendly.title, { description: formatErrorDescription(e) });
    }
  };

  const openInFinder = () => {
    if (!summary) return;
    invoke("open_in_finder", { path: summary.outputPath }).catch((e) => {
      const friendly = userErrorFrom(e);
      toast.error(friendly.title, { description: formatErrorDescription(e) });
    });
  };

  const modelReady = summarizerStatus?.modelReady ?? false;
  const available = summarizerStatus?.available ?? true;
  const unavailableReason = summarizerStatus?.unavailableReason
    ? formatErrorDescription(summarizerStatus.unavailableReason)
    : "Откройте «⚙️ Настройки» → раздел «🪶 Конспект» → нажмите «Установить окружение».";

  return (
    <div className="summary-card">
      <div className="summary-head">
        <div className="flex items-center gap-2">
          <span className="text-base">🪶</span>
          <h3 className="summary-title">Конспект</h3>
        </div>
        <div className="flex items-center gap-2">
          {modelReady && status === "done" && (
            <>
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={copySummary}
              >
                ⧉ Копировать
              </Button>
              {summary && (
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
                disabled={job.translation?.status === "generating"}
                title="Перегенерировать"
                aria-label="Перегенерировать конспект"
              >
                ↻
              </Button>
            </>
          )}
          {modelReady && status === "generating" && (
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

      {!available && (
        <Alert variant="default" className="summary-unavailable summary-state-motion">
          <AlertTitle>Окружение для конспекта не установлено</AlertTitle>
          <AlertDescription className="whitespace-pre-wrap break-words">
            {unavailableReason}
          </AlertDescription>
        </Alert>
      )}

      {available && !modelReady && !modelInstalling && (
        <div className="summary-promo summary-state-motion">
          <p className="summary-promo-text">
            Локальная модель создаёт конспекты и переводит расшифровки на русский.
            Нужно один раз скачать модель ({summaryModelSize}).
          </p>
          <Button type="button" onClick={installModel}>
            ⬇︎ Скачать модель ({summaryModelSize})
          </Button>
          {modelError && (
            <Alert variant="destructive">
              <AlertDescription className="whitespace-pre-wrap break-words">
                {modelError}
              </AlertDescription>
            </Alert>
          )}
        </div>
      )}

      {available && modelInstalling && (
        <div className="summary-progress summary-state-motion">
          <Progress
            value={Math.max(modelProgress, 2)}
            className={modelStage === "warmup" ? "animate-pulse" : ""}
          />
          <div className="summary-progress-text">
            {modelStage === "warmup"
              ? `Прогреваю модель… ${modelProgress}%`
              : `Скачиваю модель ${summaryModelSize}… ${modelProgress}%`}
          </div>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() =>
              void invoke("cancel_model_prepare", { engine: "summary" }).catch(
                () => {},
              )
            }
          >
            Отменить
          </Button>
        </div>
      )}

      {available && modelReady && status === "idle" && (
        <div className="summary-empty summary-state-motion">
          <p className="summary-empty-text">
            Соберите краткий конспект встречи: резюме, темы, тезисы и список
            действий.
          </p>
          <Button
            type="button"
            onClick={startSummary}
            disabled={!job.text || !job.outputPath || job.translation?.status === "generating"}
          >
            🪶 Сгенерировать конспект
          </Button>
        </div>
      )}

      {available && modelReady && status === "generating" && (
        <div className="summary-progress summary-state-motion">
          <Progress value={Math.max(percent, 2)} />
          <div className="summary-progress-text">
            {stageLabel(job.summary?.status === "generating" ? job.summary.stage : undefined)} {percent}%
          </div>
        </div>
      )}

      {available && modelReady && status === "error" && (
        <div className="summary-state-motion flex flex-col gap-3">
          <Alert variant="destructive">
          <AlertTitle>Не удалось создать конспект</AlertTitle>
          <AlertDescription className="whitespace-pre-wrap break-words">
              {formatErrorDescription(job.summary?.status === "error" ? job.summary.message : undefined)}
          </AlertDescription>
        </Alert>
          <Button
            type="button"
            variant="outline"
            onClick={startSummary}
            disabled={job.translation?.status === "generating"}
          >
            Попробовать ещё раз
          </Button>
        </div>
      )}

      {available && modelReady && status === "done" && rendered && (
        <article className="summary-body summary-state-motion">{rendered}</article>
      )}
    </div>
  );
}
