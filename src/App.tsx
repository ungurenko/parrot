import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { DropZone } from "./components/DropZone";
import { YouTubeInput } from "./components/YouTubeInput";
import { JobList } from "./components/JobList";
import { ResultView } from "./components/ResultView";
import { SettingsModal } from "./components/SettingsModal";
import { Onboarding } from "./components/Onboarding";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Toaster } from "@/components/ui/sonner";
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
        toast.error("Не удалось добавить файл", {
          description: String(e),
        });
      }
    }
  }, []);

  const handleYouTube = useCallback(async (url: string) => {
    try {
      await invoke("enqueue_youtube", { url });
    } catch (e) {
      toast.error("Не удалось добавить YouTube", {
        description: String(e),
      });
    }
  }, []);

  const selected = useMemo(
    () => jobs.find((j) => j.id === selectedId) ?? null,
    [jobs, selectedId],
  );

  if (needsOnboarding === null) return null;
  if (needsOnboarding) {
    return (
      <>
        <Onboarding onDone={() => setNeedsOnboarding(false)} />
        <Toaster richColors position="bottom-right" />
      </>
    );
  }

  return (
    <main className="flex h-full flex-col bg-background text-foreground">
      <header className="flex h-12 items-center justify-between border-b bg-background/85 px-4 backdrop-blur-xl">
        <div className="flex items-center gap-2">
          <span className="text-xl" aria-hidden="true">
            🦜
          </span>
          <h1 className="font-heading text-sm font-semibold">Parrot</h1>
        </div>
        <div className="flex items-center gap-2">
          {settings && (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setSettingsOpen(true)}
              title="Нажмите, чтобы сменить движок"
              className="h-7 max-w-64 text-muted-foreground"
            >
              <span aria-hidden="true">🤖</span>
              <span className="truncate">{ENGINE_LABEL[settings.engine]}</span>
            </Button>
          )}
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={() => setSettingsOpen(true)}
            title="Настройки"
            className="text-muted-foreground"
          >
            <span aria-hidden="true">⚙️</span>
            <span className="sr-only">Настройки</span>
          </Button>
        </div>
      </header>

      <div className="grid min-h-0 flex-1 grid-cols-[minmax(0,1fr)_minmax(320px,390px)] bg-background">
        <section className="flex min-h-0 flex-col gap-3 p-4">
          <DropZone onFiles={handleFiles} />
          <YouTubeInput onSubmit={handleYouTube} />
          <div className="flex min-h-0 flex-1 flex-col">
            <div className="mb-2 text-xs font-medium text-muted-foreground">
              Результат
            </div>
            <div className="min-h-0 flex-1">
              <ResultView job={selected} />
            </div>
          </div>
        </section>

        <aside className="flex min-h-0 flex-col border-l bg-muted/35 p-4">
          <div className="mb-2 text-xs font-medium text-muted-foreground">
            Очередь
          </div>
          <ScrollArea className="-mr-2 min-h-0 flex-1 pr-2">
            <JobList
              jobs={jobs}
              onSelect={(j) => setSelectedId(j.id)}
              onCancel={markCanceling}
              selectedId={selectedId}
            />
          </ScrollArea>
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
      <Toaster richColors position="bottom-right" />
    </main>
  );
}

export default App;
