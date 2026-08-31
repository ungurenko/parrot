import { useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import {
  CopyIcon,
  FolderOpenIcon,
  LanguagesIcon,
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
  type SummarizerStatus,
  type TranscriptLanguage,
} from "../types";
import { SummaryPanel } from "./SummaryPanel";
import type { SettingsTab } from "./SettingsModal";
import { TranslationStatus } from "./TranslationStatus";
import { formatErrorDescription, userErrorFrom } from "@/lib/userErrors";
import { readableEngineName } from "@/lib/engineModes";
import { localModelBusy, translationResult } from "@/lib/jobArtifacts";

interface Props {
  job: Job;
  onReset: () => void;
  engineLabel?: string;
  settings: Settings;
  onSettingsChange: (next: Settings) => void;
  onOpenSettings: (tab: SettingsTab) => void;
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
  const [activeTextView, setActiveTextView] = useState<"original" | "translation">("original");
  const summarizerEnabled = settings.summarizer_enabled;
  const translation = translationResult(job.translation);
  const modelBusy = localModelBusy(job);
  const visibleText =
    activeTextView === "translation" && translation
      ? translation.content
      : (job.text ?? "");

  const transcriptView = useMemo<TranscriptView>(() => {
    if (!visibleText) return { kind: "segments", segments: [] };
    const trimmed = visibleText.trim();
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
  }, [visibleText]);
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

  const enableLocalModelAndDownload = async () => {
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
      await navigator.clipboard.writeText(visibleText);
      toast.success(activeTextView === "translation" ? "Перевод скопирован" : "Скопировано");
    } catch (e) {
      const friendly = userErrorFrom(e);
      toast.error(friendly.title, {
        description: formatErrorDescription(e),
      });
    }
  };

  const revealInFinder = () => {
    const path = activeTextView === "translation" ? translation?.outputPath : job.outputPath;
    if (!path) return;
    invoke("open_in_finder", { path }).catch((e) => {
      const friendly = userErrorFrom(e);
      toast.error(friendly.title, { description: formatErrorDescription(e) });
    });
  };

  const startTranslation = async () => {
    if (!job.text || !job.outputPath || modelBusy) return;
    try {
      if (!summarizerEnabled) {
        const enabled = await updateSettings({
          summarizer_enabled: true,
          summarizer_promo_seen: true,
        });
        if (!enabled) return;
      }
      const status = await invoke<SummarizerStatus>("get_summarizer_status");
      if (!status.available) {
        toast.info("Нужно подготовить локальную модель", {
          description: "Откройте раздел «Локальная модель» в настройках.",
        });
        onOpenSettings("summary");
        return;
      }
      if (!status.modelReady) {
        setPendingAutoDownload(true);
        toast.info("Сначала скачаю локальную модель", {
          description: "После загрузки нажмите «Перевести на русский» ещё раз.",
        });
        window.setTimeout(() => {
          document.querySelector(".summary-card")?.scrollIntoView({ behavior: "smooth" });
        }, 0);
        return;
      }
      await invoke("translate_to_russian", {
        id: job.id,
        transcript: job.text,
        transcriptPath: job.outputPath,
      });
      setActiveTextView("translation");
    } catch (e) {
      const friendly = userErrorFrom(e);
      toast.error(friendly.title, { description: formatErrorDescription(e) });
    }
  };

  const cancelTranslation = () => {
    void invoke<boolean>("cancel_translation", { id: job.id })
      .then((canceled) => {
        if (!canceled) {
          toast.info("Перевод уже сохраняется");
        }
      })
      .catch((e) => {
        console.error("cancel_translation failed:", e);
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
        onOpenSettings("models");
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

  const wordCount = countWords(visibleText.trim());
  const charCount = visibleText.length;
  const engine = job.engine ? readableEngineName(job.engine) : (engineLabel ?? ENGINE_LABEL.parakeet);
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      <button type="button" className="ghost-btn self-start" onClick={onReset}>
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
          variant="outline"
          onClick={startTranslation}
          disabled={modelBusy}
        >
          <LanguagesIcon data-icon="inline-start" />
          {translation ? "Обновить перевод" : "Перевести на русский"}
        </Button>
        <Button
          type="button"
          variant="outline"
          className="btn-accent"
          onClick={() => {
            if (!summarizerEnabled) {
              void enableLocalModelAndDownload();
            } else {
              const card = document.querySelector(".summary-card");
              card?.scrollIntoView({
                behavior: "smooth",
                block: "nearest",
              });
              card?.classList.add("summary-flash");
              window.setTimeout(
                () => card?.classList.remove("summary-flash"),
                1200,
              );
            }
          }}
          disabled={promoBusy || job.translation?.status === "generating"}
        >
          <SparklesIcon data-icon="inline-start" />
          Сделать конспект
        </Button>
        <Button
          type="button"
          variant="ghost"
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

          {translation && (
            <div className="transcript-tabs" role="tablist" aria-label="Текст записи">
              <button
                type="button"
                role="tab"
                aria-selected={activeTextView === "original"}
                onClick={() => setActiveTextView("original")}
              >
                Оригинал
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={activeTextView === "translation"}
                onClick={() => setActiveTextView("translation")}
              >
                Перевод
              </button>
            </div>
          )}

          <TranslationStatus state={job.translation} onCancel={cancelTranslation} />

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
