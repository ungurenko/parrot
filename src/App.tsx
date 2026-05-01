import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import parrotImg from "/parrot.png";
import { EmptyState } from "./components/EmptyState";
import { ProcessingView } from "./components/ProcessingView";
import { ResultView } from "./components/ResultView";
import { JobList } from "./components/JobList";
import { SettingsModal } from "./components/SettingsModal";
import { Onboarding } from "./components/Onboarding";
import { Toaster } from "@/components/ui/sonner";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useJobEvents } from "./hooks/useJobEvents";
import { useHistory } from "./hooks/useHistory";
import { useAutoUpdate } from "./hooks/useAutoUpdate";
import { UpdateBanner } from "./components/UpdateBanner";
import {
  ENGINE_LABEL,
  type DictationPhase,
  type DictationStatus,
  type Job,
  type Settings,
} from "./types";

type ViewState =
  | { kind: "empty" }
  | { kind: "processing"; job: Job }
  | { kind: "result"; job: Job };

const BROWSER_PREVIEW_SETTINGS: Settings = {
  save_dir: "",
  onboarded: true,
  engine: "parakeet",
  language: "auto",
  summarizer_enabled: false,
  summarizer_promo_seen: true,
  dictation_enabled: true,
  dictation_hold_key: "Alt+Space",
};

function isTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

function pickView(jobs: Job[], selectedId: string | null): ViewState {
  const selected = selectedId ? jobs.find((j) => j.id === selectedId) : null;

  if (selected) {
    if (
      selected.status === "running" ||
      selected.status === "queued" ||
      selected.status === "canceling"
    ) {
      return { kind: "processing", job: selected };
    }
    return { kind: "result", job: selected };
  }

  const active = jobs.find(
    (j) =>
      j.status === "running" ||
      j.status === "queued" ||
      j.status === "canceling",
  );
  if (active) return { kind: "processing", job: active };

  return { kind: "empty" };
}

function displayShortcut(shortcut: string): string {
  return shortcut
    .split("+")
    .map((part) => (part.trim() === "Alt" ? "Option" : part.trim()))
    .join("+");
}

function App() {
  const [jobs, setJobs] = useState<Job[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [needsOnboarding, setNeedsOnboarding] = useState<boolean | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [dictationPhase, setDictationPhase] = useState<DictationPhase | "done">(
    "idle",
  );
  const [updateDismissed, setUpdateDismissed] = useState(false);
  const { history, deleteEntry, loadEntry } = useHistory();
  const updater = useAutoUpdate();

  const reloadSettings = useCallback(async () => {
    const s = await invoke<Settings>("get_settings");
    setSettings(s);
    return s;
  }, []);

  useEffect(() => {
    (async () => {
      if (!isTauriRuntime()) {
        setSettings(BROWSER_PREVIEW_SETTINGS);
        setNeedsOnboarding(false);
        setDictationPhase("idle");
        return;
      }
      const s = await reloadSettings();
      const modelReady = await invoke<boolean>("is_model_ready");
      setNeedsOnboarding(!s.onboarded || !modelReady);
      const dictation = await invoke<DictationStatus>("get_dictation_status");
      setDictationPhase(dictation.phase);
    })();
  }, [reloadSettings]);

  useEffect(() => {
    let doneTimer: number | undefined;
    const startedP = listen("dictation:started", () => {
      window.clearTimeout(doneTimer);
      setDictationPhase("recording");
    });
    const processingP = listen("dictation:processing", () => {
      window.clearTimeout(doneTimer);
      setDictationPhase("processing");
    });
    const doneP = listen<{ text: string }>("dictation:done", () => {
      setDictationPhase("done");
      toast.success("Текст вставлен");
      doneTimer = window.setTimeout(() => setDictationPhase("idle"), 1800);
    });
    const errorP = listen<{ message: string }>("dictation:error", (e) => {
      window.clearTimeout(doneTimer);
      setDictationPhase("error");
      toast.error("Диктовка не сработала", { description: e.payload.message });
    });
    return () => {
      window.clearTimeout(doneTimer);
      startedP.then((u) => u());
      processingP.then((u) => u());
      doneP.then((u) => u());
      errorP.then((u) => u());
    };
  }, []);

  useJobEvents(
    setJobs,
    useCallback((j: Job) => setSelectedId(j.id), []),
  );

  const markCanceling = useCallback((id: string) => {
    setJobs((current) =>
      current.map((job) =>
        job.id === id && (job.status === "queued" || job.status === "running")
          ? { ...job, status: "canceling" as const, stage: null }
          : job,
      ),
    );
  }, []);

  const handleFiles = useCallback(async (paths: string[]) => {
    for (const p of paths) {
      try {
        await invoke("enqueue_file", { path: p });
      } catch (e) {
        console.error("enqueue_file failed:", e);
        toast.error("Не удалось добавить файл", { description: String(e) });
      }
    }
  }, []);

  const handleYouTube = useCallback(async (url: string) => {
    try {
      await invoke("enqueue_youtube", { url });
    } catch (e) {
      toast.error("Не удалось добавить YouTube", { description: String(e) });
    }
  }, []);

  const handleOpenHistory = useCallback(
    async (id: string) => {
      try {
        const loaded = await loadEntry(id);
        if (!loaded) return;
        const rehydrated: Job = {
          id: loaded.entry.id,
          sourceName: loaded.entry.sourceName,
          status: "done",
          stage: null,
          percent: 100,
          text: loaded.text,
          outputPath: loaded.entry.outputPath,
          summary: loaded.summary,
          summaryPath: loaded.entry.summaryPath,
          summaryStatus: loaded.summary ? "done" : undefined,
          summaryPercent: loaded.summary ? 100 : undefined,
        };
        setJobs((prev) => {
          const without = prev.filter((j) => j.id !== rehydrated.id);
          return [rehydrated, ...without];
        });
        setSelectedId(rehydrated.id);
      } catch (e) {
        toast.error("Не удалось открыть запись", { description: String(e) });
      }
    },
    [loadEntry],
  );

  const view = useMemo(() => pickView(jobs, selectedId), [jobs, selectedId]);

  const hasActiveJob = useMemo(
    () =>
      jobs.some(
        (j) =>
          j.status === "running" ||
          j.status === "queued" ||
          j.status === "canceling",
      ),
    [jobs],
  );

  const resetToEmpty = useCallback(() => setSelectedId(null), []);

  if (needsOnboarding === null) return null;
  if (needsOnboarding) {
    return (
      <>
        <Onboarding onDone={() => setNeedsOnboarding(false)} />
        <Toaster richColors position="bottom-right" />
      </>
    );
  }

  const engineLabel = settings ? ENGINE_LABEL[settings.engine] : undefined;
  const showQueue = jobs.length > 1;
  const dictationLabel =
    dictationPhase === "recording"
      ? "Запись"
      : dictationPhase === "processing"
        ? "Распознаю"
        : dictationPhase === "done"
          ? "Вставлено"
          : dictationPhase === "error"
            ? "Ошибка диктовки"
            : displayShortcut(settings?.dictation_hold_key ?? "Alt+Space");
  const dictationLed =
    dictationPhase === "recording" || dictationPhase === "processing"
      ? "coral"
      : dictationPhase === "idle"
        ? "idle"
        : "";

  return (
    <main className="app-shell flex h-full flex-col">
      <header
        data-tauri-drag-region
        className="glass-toolbar flex h-[48px] items-center justify-between gap-3 px-4 pl-20"
      >
        <div data-tauri-drag-region className="flex items-center gap-2">
          <span
            className="parrot-mini"
            style={{ backgroundImage: `url(${parrotImg})` }}
            aria-hidden="true"
          />
          <h1 className="glass-title text-[13px]">Parrot</h1>
        </div>
        <div className="flex items-center gap-2">
          {settings?.dictation_enabled && (
            <span
              className="pill"
              title="Зажмите выбранное сочетание, скажите фразу и отпустите. Parrot вставит текст автоматически"
            >
              <span className={`led ${dictationLed}`} />
              <span className="truncate">{dictationLabel}</span>
            </span>
          )}
          {settings && (
            <button
              type="button"
              className="pill"
              onClick={() => setSettingsOpen(true)}
              title="Нажмите, чтобы сменить движок"
            >
              <span className="led" />
              <span className="truncate">{engineLabel}</span>
            </button>
          )}
          <button
            type="button"
            className="icon-btn"
            onClick={() => setSettingsOpen(true)}
            title="Настройки"
            aria-label="Настройки"
          >
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth={2}
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <circle cx="12" cy="12" r="3" />
              <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
            </svg>
          </button>
        </div>
      </header>

      {updater.available && !updateDismissed && (
        <UpdateBanner
          updater={updater}
          onDismiss={() => setUpdateDismissed(true)}
          onOpenSettings={() => setSettingsOpen(true)}
        />
      )}

      <div
        className={`grid min-h-0 flex-1 gap-4 p-4 ${showQueue ? "queue-grid" : ""}`}
      >
        <section className="flex min-h-0 flex-col">
          {view.kind === "empty" && (
            <EmptyState
              onFiles={handleFiles}
              onYouTube={handleYouTube}
              historyEntries={history}
              onOpenHistory={handleOpenHistory}
              onDeleteHistory={deleteEntry}
            />
          )}
          {view.kind === "processing" && (
            <ProcessingView job={view.job} onCancel={markCanceling} />
          )}
          {view.kind === "result" && settings && (
            <ResultView
              job={view.job}
              onReset={resetToEmpty}
              engineLabel={engineLabel}
              settings={settings}
              onSettingsChange={setSettings}
            />
          )}
        </section>

        {showQueue && (
          <aside className="flex min-h-0 flex-col gap-2">
            <div className="glass-label px-1">Очередь</div>
            <ScrollArea className="-mr-2 min-h-0 flex-1 pr-2">
              <JobList
                jobs={jobs}
                onSelect={(j) => setSelectedId(j.id)}
                onCancel={markCanceling}
                selectedId={selectedId}
              />
            </ScrollArea>
          </aside>
        )}
      </div>

      {settingsOpen && (
        <SettingsModal
          updater={updater}
          hasActiveJob={hasActiveJob}
          onClose={() => {
            setSettingsOpen(false);
            reloadSettings();
          }}
        />
      )}
      <Toaster richColors position="bottom-right" />
    </main>
  );
}

export default App;
