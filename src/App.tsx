import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
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
import { ENGINE_LABEL, type Job, type Settings } from "./types";

type ViewState =
  | { kind: "empty" }
  | { kind: "processing"; job: Job }
  | { kind: "result"; job: Job };

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

function App() {
  const [jobs, setJobs] = useState<Job[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [needsOnboarding, setNeedsOnboarding] = useState<boolean | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const { history, deleteEntry, loadEntry } = useHistory();

  const reloadSettings = useCallback(async () => {
    const s = await invoke<Settings>("get_settings");
    setSettings(s);
    return s;
  }, []);

  useEffect(() => {
    (async () => {
      const s = await reloadSettings();
      const modelReady = await invoke<boolean>("is_model_ready");
      setNeedsOnboarding(!s.onboarded || !modelReady);
    })();
  }, [reloadSettings]);

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
          {view.kind === "result" && (
            <ResultView
              job={view.job}
              onReset={resetToEmpty}
              engineLabel={engineLabel}
              summarizerEnabled={settings?.summarizer_enabled ?? false}
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
