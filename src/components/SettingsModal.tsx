import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
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
import { Progress } from "@/components/ui/progress";
import { Separator } from "@/components/ui/separator";
import { ENGINE_LABEL, ENGINE_SIZE, isQwenEngine, type Engine, type Settings } from "../types";
import { EnginePicker } from "./EnginePicker";
import { UpdateChecker } from "./UpdateChecker";

interface Props {
  onClose: () => void;
}

export function SettingsModal({ onClose }: Props) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [appVersion, setAppVersion] = useState("");
  const [modelReady, setModelReady] = useState(false);
  const [modelBusy, setModelBusy] = useState(false);
  const [modelError, setModelError] = useState<string | null>(null);
  const [modelProgress, setModelProgress] = useState(0);
  const [modelStage, setModelStage] = useState<"downloading" | "warmup" | "ready">(
    "downloading",
  );

  const refreshModelStatus = () =>
    invoke<boolean>("is_model_ready").then(setModelReady);

  useEffect(() => {
    invoke<Settings>("get_settings").then(setSettings);
    getVersion().then(setAppVersion).catch(() => setAppVersion(""));
    refreshModelStatus();
  }, []);

  useEffect(() => {
    const progressP = listen<number>("model:progress", (e) => {
      setModelProgress(e.payload);
    });
    const stageP = listen<"downloading" | "warmup" | "ready">(
      "model:stage",
      (e) => setModelStage(e.payload),
    );
    return () => {
      progressP.then((u) => u());
      stageP.then((u) => u());
    };
  }, []);

  const pickFolder = async () => {
    const result = await open({ directory: true, multiple: false });
    if (!result || !settings) return;
    const next = { ...settings, save_dir: result as string };
    setSettings(next);
    await invoke("set_settings", { new: next });
  };

  const changeEngine = async (engine: Engine) => {
    if (!settings) return;
    const next = { ...settings, engine };
    setSettings(next);
    setModelError(null);
    setModelProgress(0);
    await invoke("set_settings", { new: next });
    await refreshModelStatus();
  };

  const prepareModel = async () => {
    setModelBusy(true);
    setModelError(null);
    setModelProgress(1);
    setModelStage("downloading");
    try {
      await invoke("download_model");
      setModelProgress(100);
      await refreshModelStatus();
    } catch (e: any) {
      setModelError(String(e));
    } finally {
      setModelBusy(false);
    }
  };

  const openLogs = () => invoke("open_logs");

  if (!settings) return null;

  const engineLabel = ENGINE_LABEL[settings.engine];
  const engineSize = ENGINE_SIZE[settings.engine];
  const prepareLabel = isQwenEngine(settings.engine)
    ? "Подготовить модель"
    : "Скачать модель";

  return (
    <Dialog
      open
      onOpenChange={(openState) => {
        if (!openState) onClose();
      }}
    >
      <DialogContent className="flex max-h-[85vh] min-h-0 w-[min(620px,calc(100vw-2rem))] flex-col gap-0 overflow-hidden p-0 sm:max-w-none">
        <DialogHeader className="min-w-0 shrink-0 border-b bg-background/80 p-5 pr-12">
          <DialogTitle>⚙️ Настройки</DialogTitle>
          <DialogDescription>
            Движок, папка сохранения и обновления приложения.
          </DialogDescription>
        </DialogHeader>

        <FieldGroup className="min-h-0 min-w-0 flex-1 overflow-y-auto overflow-x-hidden p-5">
          <Field className="min-w-0">
            <FieldLabel>Движок транскрибации</FieldLabel>
            <EnginePicker value={settings.engine} onChange={changeEngine} />
          </Field>

          <div className="flex min-w-0 flex-col gap-3 rounded-lg bg-muted/35 p-3">
            <div className="min-w-0">
              <div className="break-words text-sm font-medium leading-snug">
                {engineLabel}
              </div>
              <div className="mt-1 text-sm leading-relaxed text-muted-foreground">
                {modelReady
                  ? `Модель готова (${engineSize})`
                  : `Нужно подготовить перед первым запуском (${engineSize})`}
              </div>
            </div>
            <div className="flex min-w-0 items-center justify-between gap-3">
              <Badge variant={modelReady ? "default" : "secondary"}>
                {modelReady ? "✅ Готова" : "Ожидает подготовки"}
              </Badge>
              {!modelReady && (
                <Button
                  onClick={prepareModel}
                  disabled={modelBusy}
                  size="sm"
                  className="shrink-0"
                >
                  {modelBusy ? `${modelProgress}%` : prepareLabel}
                </Button>
              )}
            </div>
            {modelBusy && (
              <div className="flex min-w-0 flex-col gap-2">
                <Progress
                  value={Math.max(modelProgress, 2)}
                  className={modelStage === "warmup" ? "animate-pulse" : ""}
                />
                <div className="break-words text-xs leading-relaxed text-muted-foreground">
                  {modelProgress >= 100
                    ? "✅ Готово"
                    : modelStage === "warmup"
                    ? `🔥 Прогреваю в памяти… ${modelProgress}% (разово, ~10–30 сек)`
                    : `⬇️ Скачиваю модель… ${modelProgress}%`}
                </div>
              </div>
            )}
            {modelError && (
              <Alert variant="destructive">
                <AlertDescription className="whitespace-pre-wrap break-words">
                  {modelError}
                </AlertDescription>
              </Alert>
            )}
          </div>

          <Field className="min-w-0">
            <FieldLabel htmlFor="save-dir">Папка сохранения</FieldLabel>
            <div className="flex min-w-0 items-center gap-2">
              <Input
                id="save-dir"
                readOnly
                value={settings.save_dir}
                className="min-w-0 truncate bg-background"
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

        <Separator />

        <div className="flex min-w-0 shrink-0 flex-col gap-3 bg-muted/30 p-5 sm:flex-row sm:items-center sm:justify-between sm:gap-4">
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
