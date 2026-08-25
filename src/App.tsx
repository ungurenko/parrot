import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Update } from "@tauri-apps/plugin-updater";
import { toast } from "sonner";
import { ChevronDownIcon, MicIcon } from "lucide-react";
import parrotImg from "/parrot.png";
import { EmptyState } from "./components/EmptyState";
import { ProcessingView } from "./components/ProcessingView";
import { ResultView } from "./components/ResultView";
import { JobList } from "./components/JobList";
import { SettingsModal, type SettingsTab } from "./components/SettingsModal";
import { Onboarding } from "./components/Onboarding";
import { Toaster } from "@/components/ui/sonner";
import { ScrollArea } from "@/components/ui/scroll-area";
import { jobsReducer, useJobEvents } from "./hooks/useJobEvents";
import { useHistory } from "./hooks/useHistory";
import { useAutoUpdate, type AutoUpdate } from "./hooks/useAutoUpdate";
import { useTheme } from "./hooks/useTheme";
import { UpdateBanner } from "./components/UpdateBanner";
import {
  cleanupTauriListeners,
  isTauriRuntime,
  listenInTauri,
} from "./lib/runtime";
import { modeOptionForEngine } from "./lib/engineModes";
import { formatErrorDescription, userErrorFrom } from "./lib/userErrors";
import {
  DEFAULT_SUMMARY_MODEL,
  type DictationPhase,
  type DictationStatus,
  type HistoryEntry,
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
  language: "ru",
  summarizer_enabled: false,
  summary_model: DEFAULT_SUMMARY_MODEL,
  summarizer_promo_seen: true,
  dictation_enabled: true,
  dictation_hold_key: "Alt+Space",
  theme: "system",
};

const PREVIEW_NOTES = [
  "## 🎯 Что нового",
  "",
  "Транскрибация стала заметно быстрее: модель теперь постоянно живёт в памяти, поэтому файлы и голосовые начинаются сразу, без паузы на загрузку.",
  "",
  "Parrot сам подстраивается под конкретный Mac — на машинах с 16 ГБ памяти работает быстрый режим на видеокарте.",
  "",
  "Прогресс на длинных записях теперь двигается по ходу обработки.",
  "",
  "## 📦 Как получить обновление",
  "",
  "Откройте Parrot и установите обновление из появившегося уведомления.",
].join("\n");

// Browser preview only: lets the update banner render outside Tauri.
const PREVIEW_UPDATER: AutoUpdate = {
  available: { version: "0.4.27", body: PREVIEW_NOTES } as unknown as Update,
  status: "idle",
  progress: 0,
  errorDetails: null,
  errorScope: null,
  runCheck: async () => {},
  install: async () => {},
};

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

const SHORTCUT_GLYPH: Record<string, string> = {
  Cmd: "⌘",
  Command: "⌘",
  Meta: "⌘",
  Shift: "⇧",
  Option: "⌥",
  Alt: "⌥",
  Ctrl: "⌃",
  Control: "⌃",
};

function parseShortcut(shortcut: string): string[] {
  return shortcut
    .split("+")
    .map((part) => part.trim())
    .filter(Boolean)
    .map((part) => SHORTCUT_GLYPH[part] ?? part.toUpperCase());
}

function pluralFiles(n: number): string {
  const mod10 = n % 10;
  const mod100 = n % 100;
  if (mod10 === 1 && mod100 !== 11) return "файл";
  if (mod10 >= 2 && mod10 <= 4 && (mod100 < 12 || mod100 > 14)) return "файла";
  return "файлов";
}

function App() {
  const [jobs, dispatchJobs] = useReducer(jobsReducer, []);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [historyJob, setHistoryJob] = useState<Job | null>(null);
  const selectedIdRef = useRef<string | null>(selectedId);
  selectedIdRef.current = selectedId;
  const jobsRef = useRef(jobs);
  jobsRef.current = jobs;
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsTab, setSettingsTab] = useState<SettingsTab>("basic");
  const [needsOnboarding, setNeedsOnboarding] = useState<boolean | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [dictationPhase, setDictationPhase] = useState<DictationPhase | "done">(
    "idle",
  );
  const [updateDismissed, setUpdateDismissed] = useState(false);
  const { history, deleteEntry, clearAll, loadEntry } = useHistory();
  const updater = useAutoUpdate();
  const resolvedTheme = useTheme(settings?.theme);

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
    if (!isTauriRuntime()) return;
    let doneTimer: number | undefined;
    const listeners = [
      listenInTauri("dictation:started", () => {
        window.clearTimeout(doneTimer);
        setDictationPhase("recording");
      }),
      listenInTauri("dictation:processing", () => {
        window.clearTimeout(doneTimer);
        setDictationPhase("processing");
      }),
      listenInTauri<{ text: string }>("dictation:done", () => {
        setDictationPhase("done");
        toast.success("Текст вставлен");
        doneTimer = window.setTimeout(() => setDictationPhase("idle"), 1800);
      }),
      listenInTauri<{ message: string }>("dictation:error", (e) => {
        window.clearTimeout(doneTimer);
        setDictationPhase("error");
        const friendly = userErrorFrom(e.payload.message);
        toast.error(friendly.title, {
          description: formatErrorDescription(e.payload.message),
        });
      }),
      listenInTauri<string>("parakeet_mlx:progress", (e) => {
        if (
          e.payload.includes("Устанавливаю") ||
          e.payload.includes("Скачиваю")
        ) {
          toast.message("Готовлю ускорение распознавания", {
            id: "parakeet-mlx",
            description: e.payload,
          });
        }
      }),
      listenInTauri("parakeet_mlx:ready", () => {
        toast.success("Распознавание ускорено", {
          id: "parakeet-mlx",
          description: "Следующие файлы пойдут быстрее",
        });
      }),
    ];
    return () => {
      window.clearTimeout(doneTimer);
      cleanupTauriListeners(listeners);
    };
  }, []);

  useJobEvents(
    dispatchJobs,
    useCallback((id: string) => {
      const selected = selectedIdRef.current
        ? jobsRef.current.find((j) => j.id === selectedIdRef.current)
        : null;
      if (!selected || selected.status !== "done") {
        setSelectedId(id);
        return;
      }
      const finished = jobsRef.current.find((j) => j.id === id);
      toast.success("Расшифровка готова", {
        description: finished?.sourceName,
      });
    }, []),
    useCallback((id: string, message: string) => {
      if (selectedIdRef.current === id) return;
      const friendly = userErrorFrom(message);
      toast.error(friendly.title, {
        description: formatErrorDescription(message),
      });
    }, []),
  );

  const cancelJob = useCallback(async (id: string) => {
    try {
      await invoke("cancel_job", { id });
      dispatchJobs({ type: "jobCanceling", id });
    } catch (error) {
      console.error("cancel_job failed:", error);
      const friendly = userErrorFrom(error);
      toast.error(friendly.title, {
        description: formatErrorDescription(error),
      });
    }
  }, []);

  const handleFiles = useCallback(async (paths: string[]) => {
    if (!isTauriRuntime()) return;
    let queued = 0;
    for (const p of paths) {
      try {
        await invoke("enqueue_file", { path: p });
        queued += 1;
      } catch (e) {
        console.error("enqueue_file failed:", e);
        const friendly = userErrorFrom(e);
        toast.error(friendly.title, { description: formatErrorDescription(e) });
      }
    }
    if (queued === 1) {
      toast.success("Добавил файл, начинаю…");
    } else if (queued > 1) {
      toast.success(`Добавил ${queued} ${pluralFiles(queued)} в очередь`);
    }
  }, []);

  const handleYouTube = useCallback(async (url: string): Promise<boolean> => {
    if (!isTauriRuntime()) return false;
    try {
      await invoke("enqueue_youtube", { url });
      toast.success("Добавил ссылку, начинаю…");
      return true;
    } catch (e) {
      const friendly = userErrorFrom(e);
      toast.error(friendly.title, { description: formatErrorDescription(e) });
      return false;
    }
  }, []);

  const handleOpenHistory = useCallback(
    async (id: string) => {
      try {
        const loaded = await loadEntry(id);
        if (!loaded) {
          toast.info("Запись недоступна");
          return;
        }
        const rehydrated: Job = {
          id: loaded.entry.id,
          sourceName: loaded.entry.sourceName,
          sourceKind: loaded.entry.sourceKind,
          sourceValue: loaded.entry.sourceValue,
          status: "done",
          stage: null,
          percent: 100,
          text: loaded.text,
          outputPath: loaded.entry.outputPath,
          engine: loaded.entry.engine as Job["engine"],
          language: loaded.entry.language as Job["language"],
          summary: loaded.summary,
          summaryPath: loaded.entry.summaryPath,
          summaryStatus: loaded.summary ? "done" : undefined,
          summaryPercent: loaded.summary ? 100 : undefined,
        };
        setHistoryJob(rehydrated);
        setSelectedId(rehydrated.id);
      } catch (e) {
        const friendly = userErrorFrom(e);
        toast.error(friendly.title, { description: formatErrorDescription(e) });
      }
    },
    [loadEntry],
  );

  const handleRepeatHistory = useCallback(async (entry: HistoryEntry) => {
    if (!entry.sourceKind || !entry.sourceValue) {
      toast.info("Повтор недоступен", {
        description: "Эта запись создана в старой версии истории. Добавьте файл заново.",
      });
      return;
    }

    try {
      if (entry.sourceKind === "localFile") {
        await invoke("enqueue_file", {
          path: entry.sourceValue,
          engine: entry.engine,
          language: entry.language,
        });
      } else {
        await invoke("enqueue_youtube", {
          url: entry.sourceValue,
          engine: entry.engine,
          language: entry.language,
        });
      }
      toast.success("Запустил повтор", {
        description: "Запись добавлена в очередь.",
      });
    } catch (e) {
      const friendly = userErrorFrom(e);
      toast.error(friendly.title, { description: formatErrorDescription(e) });
    }
  }, []);

  const handleDeleteHistory = useCallback(
    async (id: string) => {
      try {
        await deleteEntry(id);
      } catch (e) {
        const friendly = userErrorFrom(e);
        toast.error(friendly.title, { description: formatErrorDescription(e) });
      }
    },
    [deleteEntry],
  );

  const view = useMemo(() => pickView(historyJob ? [historyJob, ...jobs] : jobs, selectedId), [historyJob, jobs, selectedId]);

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

  const queueJobs = useMemo(
    () => (historyJob ? jobs.filter((j) => j.id !== historyJob.id) : jobs),
    [jobs, historyJob],
  );

  const openSettings = useCallback((tab: SettingsTab = "basic") => {
    setSettingsTab(tab);
    setSettingsOpen(true);
  }, []);

  const resetToEmpty = useCallback(() => setSelectedId(null), []);

  if (needsOnboarding === null) return <main className="app-shell h-full" />;
  if (needsOnboarding) {
    return (
      <>
        <Onboarding onDone={() => setNeedsOnboarding(false)} />
        <Toaster theme={resolvedTheme} richColors position="bottom-right" />
      </>
    );
  }

  const engineLabel = settings ? modeOptionForEngine(settings.engine).title : undefined;
  const dictationStatus =
    dictationPhase === "recording"
      ? "Запись"
      : dictationPhase === "processing"
        ? "Распознаю"
        : dictationPhase === "done"
          ? "Вставлено"
          : dictationPhase === "error"
            ? "Ошибка диктовки"
            : "Готово к диктовке";
  const showQueue = queueJobs.length > 1;
  const dictationIdle =
    dictationPhase === "idle" ||
    dictationPhase === "done" ||
    dictationPhase === "error";
  const dictationKeys = parseShortcut(
    settings?.dictation_hold_key ?? "Alt+Space",
  );
  const dictationTitle = `Зажмите ${displayShortcut(settings?.dictation_hold_key ?? "Alt+Space")}, скажите фразу и отпустите. Parrot вставит текст автоматически`;
  const dictationLed =
    dictationPhase === "recording" || dictationPhase === "processing"
      ? "coral"
      : dictationPhase === "idle"
        ? "idle"
        : "";

  const bannerUpdater = isTauriRuntime() ? updater : PREVIEW_UPDATER;

  return (
    <main className="app-shell flex h-full flex-col">
      <header
        data-tauri-drag-region
        className="glass-toolbar flex items-center justify-between gap-3 px-4 pl-20"
      >
        <div data-tauri-drag-region className="flex items-center gap-2">
          <span
            className="parrot-mini"
            style={{ backgroundImage: `url(${parrotImg})` }}
            aria-hidden="true"
          />
          <h1 className="glass-title text-[13px]">Parrot</h1>
        </div>
        <div className="toolbar-actions flex min-w-0 items-center gap-2">
          {settings?.dictation_enabled && (
            <button
              type="button"
              className={`pill dictation-pill${dictationIdle ? " idle" : ""}`}
              aria-label={dictationTitle}
              title={dictationTitle}
              onClick={() => openSettings("dictation")}
            >
              <MicIcon
                size={14}
                className="dictation-mic"
                aria-hidden="true"
              />
              {dictationIdle && (
                <span className="dictation-keys">
                  {dictationKeys.map((key, idx) => (
                    <kbd key={idx} className="dictation-kbd">
                      {key}
                    </kbd>
                  ))}
                </span>
              )}
              <span className="dictation-status truncate">
                {dictationStatus}
              </span>
              <span className={`led ${dictationLed}`} />
            </button>
          )}
          {settings && (
            <button
              type="button"
              className="pill engine-pill"
              onClick={() => openSettings("basic")}
              title="Открыть настройки распознавания"
            >
              <span className="truncate">{engineLabel}</span>
              <ChevronDownIcon
                size={14}
                className="pill-chevron"
                aria-hidden="true"
              />
            </button>
          )}
        </div>
      </header>

      {bannerUpdater.available && !updateDismissed && (
        <UpdateBanner
          updater={bannerUpdater}
          onDismiss={() => setUpdateDismissed(true)}
          onOpenSettings={() => openSettings("updates")}
        />
      )}

      <div
        className={`grid min-h-0 flex-1 gap-4 p-4 ${showQueue ? "queue-grid" : ""}`}
      >
        <section className="flex min-h-0 min-w-0 flex-col">
          <div key={view.kind} className="view-enter flex min-h-0 flex-1 flex-col">
          {view.kind === "empty" && (
            <EmptyState
              onFiles={handleFiles}
              onYouTube={handleYouTube}
              historyEntries={history}
              onOpenHistory={handleOpenHistory}
              onDeleteHistory={handleDeleteHistory}
              onClearHistory={() => {
                clearAll();
                toast.success("История очищена");
              }}
              onRepeatHistory={handleRepeatHistory}
            />
          )}
          {view.kind === "processing" && (
            <ProcessingView job={view.job} onCancel={cancelJob} />
          )}
          {view.kind === "result" && settings && (
            <ResultView
              job={view.job}
              onReset={resetToEmpty}
              engineLabel={engineLabel}
              settings={settings}
              onSettingsChange={setSettings}
              onOpenSettings={() => openSettings("models")}
            />
          )}
          </div>
        </section>

        {showQueue && (
          <aside className="flex min-h-0 flex-col gap-2">
            <div className="glass-label px-1">Очередь</div>
            <ScrollArea className="-mr-2 min-h-0 flex-1 pr-2">
              <JobList
                jobs={queueJobs}
                onSelect={(j) => setSelectedId(j.id)}
                onCancel={cancelJob}
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
          initialTab={settingsTab}
          onClose={() => {
            setSettingsOpen(false);
            if (isTauriRuntime()) {
              reloadSettings();
            }
          }}
        />
      )}
      <Toaster theme={resolvedTheme} richColors position="bottom-right" />
    </main>
  );
}

export default App;
