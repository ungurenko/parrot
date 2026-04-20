import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Field,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Separator } from "@/components/ui/separator";
import {
  ENGINE_LABEL,
  LANGUAGE_LABEL,
  type Engine,
  type EngineStatuses,
  type ModelProgressDetail,
  type Settings,
  type TranscriptLanguage,
} from "../types";
import { EnginePicker } from "./EnginePicker";
import { UpdateChecker } from "./UpdateChecker";

interface Props {
  onClose: () => void;
}

export function SettingsModal({ onClose }: Props) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [appVersion, setAppVersion] = useState("");
  const [modelStatuses, setModelStatuses] = useState<EngineStatuses>({});
  const [busyEngine, setBusyEngine] = useState<Engine | null>(null);
  const [deletingEngine, setDeletingEngine] = useState<Engine | null>(null);
  const [modelError, setModelError] = useState<string | null>(null);
  const [modelProgress, setModelProgress] = useState(0);
  const [modelProgressDetail, setModelProgressDetail] =
    useState<ModelProgressDetail | null>(null);
  const [modelStage, setModelStage] = useState<"downloading" | "warmup" | "ready">(
    "downloading",
  );

  const refreshModelStatuses = () =>
    invoke<EngineStatuses>("get_engine_statuses").then(setModelStatuses);

  useEffect(() => {
    invoke<Settings>("get_settings").then(setSettings);
    getVersion().then(setAppVersion).catch(() => setAppVersion(""));
    refreshModelStatuses();
  }, []);

  useEffect(() => {
    const progressP = listen<number>("model:progress", (e) => {
      setModelProgress(e.payload);
    });
    const progressDetailP = listen<ModelProgressDetail>(
      "model:progress_detail",
      (e) => setModelProgressDetail(e.payload),
    );
    const stageP = listen<"downloading" | "warmup" | "ready">(
      "model:stage",
      (e) => setModelStage(e.payload),
    );
    return () => {
      progressP.then((u) => u());
      progressDetailP.then((u) => u());
      stageP.then((u) => u());
    };
  }, []);

  const pickFolder = async () => {
    const result = await open({ directory: true, multiple: false });
    if (!result || !settings) return;
    const next = { ...settings, save_dir: result as string };
    try {
      await invoke("set_settings", { new: next });
      setSettings(next);
      setModelError(null);
    } catch (e: unknown) {
      setModelError(String(e));
    }
  };

  const changeEngine = async (engine: Engine) => {
    if (!settings) return;
    const next = { ...settings, engine };
    try {
      await invoke("set_settings", { new: next });
      setSettings(next);
      setModelError(null);
    } catch (e: unknown) {
      setModelError(String(e));
    }
  };

  const changeLanguage = async (language: TranscriptLanguage) => {
    if (!settings) return;
    const next = { ...settings, language };
    try {
      await invoke("set_settings", { new: next });
      setSettings(next);
      setModelError(null);
    } catch (e: unknown) {
      setModelError(String(e));
    }
  };

  const prepareModel = async (engine: Engine) => {
    setBusyEngine(engine);
    setModelError(null);
    setModelProgress(1);
    setModelProgressDetail(null);
    setModelStage("downloading");
    try {
      await invoke("download_model_for_engine", { engine });
      setModelProgress(100);
      await refreshModelStatuses();
    } catch (e: unknown) {
      setModelError(String(e));
    } finally {
      setBusyEngine(null);
    }
  };

  const deleteModel = async (engine: Engine) => {
    if (!modelStatuses[engine]?.modelReady) return;
    const ok = window.confirm(
      `Вы уверены, что хотите удалить модель «${ENGINE_LABEL[engine]}»?\n\nТексты и настройки останутся на месте. Если модель понадобится снова, ее можно будет скачать заново.`,
    );
    if (!ok) return;

    setDeletingEngine(engine);
    setModelError(null);
    try {
      await invoke("delete_model_for_engine", { engine });
      await refreshModelStatuses();
      setModelProgress(0);
      setModelProgressDetail(null);
    } catch (e: unknown) {
      setModelError(String(e));
      await refreshModelStatuses();
    } finally {
      setDeletingEngine(null);
    }
  };

  const openLogs = () => invoke("open_logs");

  if (!settings) return null;

  return (
    <Dialog
      open
      onOpenChange={(openState) => {
        if (!openState) onClose();
      }}
    >
      <DialogContent className="glass-modal flex max-h-[85vh] min-h-0 w-[min(620px,calc(100vw-2rem))] flex-col gap-0 overflow-hidden p-0 sm:max-w-none">
        <DialogHeader className="modal-band-top min-w-0 shrink-0 p-5 pr-12">
          <DialogTitle>⚙️ Настройки</DialogTitle>
          <DialogDescription>
            Движок, папка сохранения и обновления приложения.
          </DialogDescription>
        </DialogHeader>

        <FieldGroup className="min-h-0 min-w-0 flex-1 overflow-y-auto overflow-x-hidden p-5">
          <Field className="min-w-0">
            <FieldLabel>Модели распознавания</FieldLabel>
            <EnginePicker
              value={settings.engine}
              statuses={modelStatuses}
              busyEngine={busyEngine}
              deletingEngine={deletingEngine}
              progress={modelProgress}
              progressDetail={modelProgressDetail}
              stage={modelStage}
              onChange={changeEngine}
              onPrepare={prepareModel}
              onDelete={deleteModel}
            />
          </Field>

          <Field className="min-w-0">
            <FieldLabel htmlFor="language">Язык аудио</FieldLabel>
            <select
              id="language"
              value={settings.language}
              onChange={(e) => changeLanguage(e.target.value as TranscriptLanguage)}
              className="h-10 w-full rounded-lg border border-white/70 bg-white/55 px-3 text-sm text-[color:var(--ink)] shadow-[inset_0_1px_0_rgba(255,255,255,0.85)] outline-none transition-all duration-200 focus-visible:border-[color:var(--parrot-accent)] focus-visible:bg-white/85 focus-visible:ring-3 focus-visible:ring-[color:rgba(255,122,89,0.28)]"
            >
              {Object.entries(LANGUAGE_LABEL).map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
          </Field>

          {modelError && (
            <Alert variant="destructive">
              <AlertDescription className="whitespace-pre-wrap break-words">
                {modelError}
              </AlertDescription>
            </Alert>
          )}

          <Field className="min-w-0">
            <FieldLabel htmlFor="save-dir">Папка сохранения</FieldLabel>
            <div className="flex min-w-0 items-center gap-2">
              <Input
                id="save-dir"
                readOnly
                value={settings.save_dir}
                className="glass-input min-w-0 truncate"
              />
              <Button
                type="button"
                variant="outline"
                onClick={pickFolder}
                className="shrink-0"
              >
                Выбрать…
              </Button>
            </div>
          </Field>
        </FieldGroup>

        <Separator className="bg-white/50" />

        <div className="modal-band-bottom flex min-w-0 shrink-0 flex-col gap-3 p-5 sm:flex-row sm:items-center sm:justify-between sm:gap-4">
          <div className="flex min-w-0 flex-col items-start gap-1">
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={openLogs}
              className="px-0 text-muted-foreground"
            >
              📜 Открыть логи
            </Button>
            <UpdateChecker />
          </div>
          <div className="min-w-0 text-left text-xs text-muted-foreground sm:shrink-0 sm:text-right">
            <div>Разработано Александром Унгуренко, 2026</div>
            {appVersion && <div>v{appVersion}</div>}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
