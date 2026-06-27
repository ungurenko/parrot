import { useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import {
  CopyIcon,
  FolderOpenIcon,
  RefreshCwIcon,
  SparklesIcon,
} from "lucide-react";
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
  type EngineStatuses,
  type Job,
  type Settings,
  type TranscriptLanguage,
} from "../types";
import { SummaryPanel } from "./SummaryPanel";
import { formatErrorDescription, userErrorFrom } from "@/lib/userErrors";
import { readableEngineName } from "@/lib/engineModes";

interface Props {
  job: Job;
  onReset: () => void;
  engineLabel?: string;
  settings: Settings;
  onSettingsChange: (next: Settings) => void;
  onOpenSettings: () => void;
}

const PLAIN_TRANSCRIPT_CHAR_LIMIT = 50_000;
const PLAIN_TRANSCRIPT_LINE_LIMIT = 300;

type TranscriptView =
  | { kind: "segments"; segments: string[] }
  | { kind: "plain"; text: string };

function countWords(text: string): number {
  let count = 0;
  for (const _match of text.matchAll(/\S+/g)) count += 1;
  return count;
}

export function ResultView({
  job,
  onReset,
  engineLabel,
  settings,
  onSettingsChange,
  onOpenSettings,
}: Props) {
  const [transcriptExpanded, setTranscriptExpanded] = useState(false);
  const [pendingAutoDownload, setPendingAutoDownload] = useState(false);
  const [promoBusy, setPromoBusy] = useState(false);
  const summarizerEnabled = settings.summarizer_enabled;

  const transcriptView = useMemo<TranscriptView>(() => {
    if (!job.text) return { kind: "segments", segments: [] };
    const trimmed = job.text.trim();
    const lineCount = (trimmed.match(/\n/g)?.length ?? 0) + 1;
    if (
      trimmed.length > PLAIN_TRANSCRIPT_CHAR_LIMIT ||
      lineCount > PLAIN_TRANSCRIPT_LINE_LIMIT
    ) {
      return { kind: "plain", text: trimmed };
    }
    const split = trimmed.split(/\n{2,}/g).map((s) => s.trim()).filter(Boolean);
    const segments =
      split.length > 1
        ? split
        : trimmed
            .split(/\n/g)
            .map((s) => s.trim())
            .filter(Boolean);
    return { kind: "segments", segments };
  }, [job.text]);
  const segments =
    transcriptView.kind === "segments" ? transcriptView.segments : [];

  const updateSettings = async (patch: Partial<Settings>) => {
    const next = { ...settings, ...patch };
    try {
      await invoke("set_settings", { new: next });
      onSettingsChange(next);
      return true;
    } catch (e) {
      toast.error("Не удалось сохранить настройку", {
        description: formatErrorDescription(e),
      });
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

  if (job.status === "error") {
    const friendly = userErrorFrom(job.error);
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
          <AlertTitle>{friendly.title}</AlertTitle>
          <AlertDescription className="whitespace-pre-wrap">
            {formatErrorDescription(job.error)}
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
      const friendly = userErrorFrom(e);
      toast.error(friendly.title, {
        description: formatErrorDescription(e),
      });
    }
  };

  const revealInFinder = () => {
    if (!job.outputPath) return;
    invoke("open_in_finder", { path: job.outputPath }).catch((e) => {
      const friendly = userErrorFrom(e);
      toast.error(friendly.title, { description: formatErrorDescription(e) });
    });
  };

  const improveQuality = async () => {
    if (!job.sourceKind || !job.sourceValue) {
      toast.info("Повтор недоступен", {
        description: "Эта запись создана в старой версии истории. Добавьте файл заново.",
      });
      return;
    }

    try {
      const statuses = await invoke<EngineStatuses>("get_engine_statuses");
      const qwenStatus = statuses["qwen-0.6b"];
      if (!qwenStatus?.available || !qwenStatus.modelReady) {
        toast.info("Нужно подготовить режим качества", {
          description:
            "Откройте настройки, скачайте режим «Лучше для русского» и запустите улучшение ещё раз.",
        });
        onOpenSettings();
        return;
      }

      const language = (job.language ?? settings.language) as TranscriptLanguage;
      if (job.sourceKind === "localFile") {
        await invoke("enqueue_file", {
          path: job.sourceValue,
          engine: "qwen-0.6b",
          language,
        });
      } else {
        await invoke("enqueue_youtube", {
          url: job.sourceValue,
          engine: "qwen-0.6b",
          language,
        });
      }
      toast.success("Запустил улучшение качества", {
        description: "Parrot повторит эту запись в режиме «Лучше для русского».",
      });
    } catch (e) {
      const friendly = userErrorFrom(e);
      toast.error(friendly.title, { description: formatErrorDescription(e) });
    }
  };

  const wordCount = countWords((job.text ?? "").trim());
  const charCount = (job.text ?? "").length;
  const engine = job.engine ? readableEngineName(job.engine) : (engineLabel ?? ENGINE_LABEL.parakeet);
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

      <div className="result-action-bar">
        <Button type="button" onClick={copyText}>
          <CopyIcon data-icon="inline-start" />
          Скопировать
        </Button>
        <Button
          type="button"
          variant="outline"
          onClick={revealInFinder}
          disabled={!job.outputPath}
        >
          <FolderOpenIcon data-icon="inline-start" />
          Показать файл
        </Button>
        <Button
          type="button"
          variant={summarizerEnabled ? "outline" : "default"}
          onClick={() => {
            if (!summarizerEnabled) {
              void enableSummarizerAndDownload();
            } else {
              document.querySelector(".summary-card")?.scrollIntoView({
                behavior: "smooth",
                block: "nearest",
              });
            }
          }}
          disabled={promoBusy}
        >
          <SparklesIcon data-icon="inline-start" />
          Сделать конспект
        </Button>
        <Button
          type="button"
          variant="outline"
          onClick={improveQuality}
          disabled={!job.sourceKind || !job.sourceValue}
          title={
            job.sourceKind && job.sourceValue
              ? "Повторить эту запись в режиме «Лучше для русского»"
              : "Повтор недоступен для старой записи истории"
          }
        >
          <RefreshCwIcon data-icon="inline-start" />
          Улучшить качество
        </Button>
      </div>

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
          </div>

          <div
            className={`segments ${
              summarizerEnabled && !transcriptExpanded ? "segments-collapsed" : ""
            }`}
          >
            {transcriptView.kind === "plain" ? (
              <pre className="seg transcript-plain">{transcriptView.text}</pre>
            ) : segments.length > 0 ? (
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

        <SummaryPanel
          job={job}
          settings={settings}
          autoStartDownload={pendingAutoDownload}
        />
      </div>
    </div>
  );
}
