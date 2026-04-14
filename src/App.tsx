import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { DropZone } from "./components/DropZone";
import { YouTubeInput } from "./components/YouTubeInput";
import { JobList } from "./components/JobList";
import { ResultView } from "./components/ResultView";
import { SettingsModal } from "./components/SettingsModal";
import { Onboarding } from "./components/Onboarding";
import { useJobEvents } from "./hooks/useJobEvents";
import { ENGINE_LABEL, type Job, type Settings } from "./types";

function App() {
  const [jobs, setJobs] = useState<Job[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [needsOnboarding, setNeedsOnboarding] = useState<boolean | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);

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

  useJobEvents(setJobs, useCallback((j: Job) => setSelectedId(j.id), []));

  const handleFiles = useCallback(async (paths: string[]) => {
    for (const p of paths) {
      try {
        await invoke("enqueue_file", { path: p });
      } catch (e) {
        console.error("enqueue_file failed:", e);
        alert(`Не удалось добавить файл:\n${e}`);
      }
    }
  }, []);

  const handleYouTube = useCallback(async (url: string) => {
    try {
      await invoke("enqueue_youtube", { url });
    } catch (e) {
      alert(`Не удалось добавить YouTube:\n${e}`);
    }
  }, []);

  const selected = useMemo(
    () => jobs.find((j) => j.id === selectedId) ?? null,
    [jobs, selectedId],
  );

  if (needsOnboarding === null) return null;
  if (needsOnboarding) {
    return <Onboarding onDone={() => setNeedsOnboarding(false)} />;
  }

  return (
    <main className="flex flex-col h-full">
      <header className="flex items-center justify-between px-5 py-3 border-b border-[var(--color-border)]">
        <div className="flex items-center gap-2">
          <span className="text-xl">🦜</span>
          <h1 className="font-semibold">Parrot</h1>
        </div>
        <div className="flex items-center gap-3">
          {settings && (
            <button
              onClick={() => setSettingsOpen(true)}
              className="text-xs text-[var(--color-muted)] hover:text-[var(--color-text)] px-2 py-1 rounded border border-[var(--color-border)] hover:bg-[var(--color-panel-hover)]"
              title="Нажмите, чтобы сменить движок"
            >
              🤖 {ENGINE_LABEL[settings.engine]}
            </button>
          )}
          <button
            onClick={() => setSettingsOpen(true)}
            className="text-lg hover:opacity-80"
            title="Настройки"
          >
            ⚙️
          </button>
        </div>
      </header>

      <div className="flex-1 grid grid-cols-[minmax(0,1fr)_minmax(320px,420px)] gap-4 p-5 min-h-0">
        <section className="flex flex-col gap-4 min-h-0">
          <DropZone onFiles={handleFiles} />
          <YouTubeInput onSubmit={handleYouTube} />
          <div className="flex-1 min-h-0 flex flex-col">
            <div className="text-xs uppercase tracking-wide text-[var(--color-muted)] mb-2">
              Результат
            </div>
            <div className="flex-1 min-h-0">
              <ResultView job={selected} />
            </div>
          </div>
        </section>

        <aside className="flex flex-col min-h-0">
          <div className="text-xs uppercase tracking-wide text-[var(--color-muted)] mb-2">
            Очередь
          </div>
          <div className="flex-1 min-h-0 overflow-y-auto pr-1">
            <JobList
              jobs={jobs}
              onSelect={(j) => setSelectedId(j.id)}
              selectedId={selectedId}
            />
          </div>
        </aside>
      </div>

      {settingsOpen && (
        <SettingsModal
          onClose={() => {
            setSettingsOpen(false);
            reloadSettings();
          }}
        />
      )}
    </main>
  );
}

export default App;
