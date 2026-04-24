import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import parrotImg from "/parrot.png";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Progress } from "@/components/ui/progress";
import {
  isSlowModelDownload,
  modelDownloadDetails,
} from "@/lib/modelProgress";
import {
  ENGINE_LABEL,
  ENGINE_SIZE,
  type Engine,
  type EngineStatuses,
  type ModelProgressDetail,
  type Settings,
} from "../types";
import { EnginePicker } from "./EnginePicker";

interface Props {
  onDone: () => void;
}

type Step = "folder" | "engine" | "model" | "downloading" | "ready";
type ModelStage = "downloading" | "warmup" | "ready";

export function Onboarding({ onDone }: Props) {
  const [step, setStep] = useState<Step>("folder");
  const [settings, setSettings] = useState<Settings | null>(null);
  const [engineStatuses, setEngineStatuses] = useState<EngineStatuses>({});
  const [progress, setProgress] = useState(0);
  const [progressDetail, setProgressDetail] =
    useState<ModelProgressDetail | null>(null);
  const [modelStage, setModelStage] = useState<ModelStage>("downloading");
  const [error, setError] = useState<string | null>(null);
  const selectedEngineStatus = settings ? engineStatuses[settings.engine] : undefined;

  useEffect(() => {
    invoke<Settings>("get_settings").then(setSettings);
    invoke<EngineStatuses>("get_engine_statuses").then(setEngineStatuses);
  }, []);

  useEffect(() => {
    const progressP = listen<number>("model:progress", (e) =>
      setProgress(e.payload),
    );
    const progressDetailP = listen<ModelProgressDetail>(
      "model:progress_detail",
      (e) => setProgressDetail(e.payload),
    );
    const stageP = listen<ModelStage>("model:stage", (e) =>
      setModelStage(e.payload),
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
      setError(null);
    } catch (e: unknown) {
      setError(String(e));
    }
  };

  const changeEngine = async (engine: Engine) => {
    if (!settings) return;
    const next = { ...settings, engine };
    try {
      await invoke("set_settings", { new: next });
      setSettings(next);
      setError(null);
    } catch (e: unknown) {
      setError(String(e));
    }
  };

  const downloadModel = async () => {
    setStep("downloading");
    setModelStage("downloading");
    setProgress(0);
    setProgressDetail(null);
    setError(null);
    try {
      await invoke("download_model");
      invoke<EngineStatuses>("get_engine_statuses").then(setEngineStatuses);
      setStep("ready");
    } catch (e: unknown) {
      setError(String(e));
      setStep("model");
    }
  };

  const finish = async () => {
    if (settings) {
      await invoke("set_settings", {
        new: { ...settings, language: settings.language ?? "auto", onboarded: true },
      });
    }
    onDone();
  };

  return (
    <main className="app-shell fixed inset-0 flex items-center justify-center overflow-y-auto p-6">
      <Card className="glass-modal w-full max-w-2xl border-0 p-0 shadow-none">
        <CardHeader className="p-6 pb-4">
          <CardTitle className="flex items-center gap-3 text-2xl">
            <span
              className="parrot-mini-lg"
              style={{ backgroundImage: `url(${parrotImg})` }}
              aria-hidden="true"
            />
            Parrot
          </CardTitle>
          <CardDescription>
            Настроим приложение перед первым запуском.
          </CardDescription>
        </CardHeader>

        <CardContent className="px-6 pb-6">
          {error && step !== "model" && (
            <Alert variant="destructive" className="mb-4">
              <AlertDescription className="whitespace-pre-wrap">
                {error}
              </AlertDescription>
            </Alert>
          )}

          {step === "folder" && settings && (
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor="onboarding-save-dir">
                  1. Выберите папку для транскрипций
                </FieldLabel>
                <div className="flex items-center gap-2">
                  <Input
                    id="onboarding-save-dir"
                    readOnly
                    value={settings.save_dir}
                    className="truncate bg-background"
                  />
                  <Button
                    type="button"
                    variant="outline"
                    onClick={pickFolder}
                    className="shrink-0"
                  >
                    Изменить…
                  </Button>
                </div>
              </Field>
            </FieldGroup>
          )}

          {step === "engine" && settings && (
            <FieldGroup>
              <Field>
                <FieldLabel>2. Выберите движок транскрибации</FieldLabel>
                <FieldDescription>
                  Можно будет поменять в настройках.
                </FieldDescription>
                <EnginePicker
                  value={settings.engine}
                  statuses={engineStatuses}
                  onChange={changeEngine}
                />
              </Field>
            </FieldGroup>
          )}

          {step === "model" && settings && (
            <FieldGroup>
              <Field>
                <FieldLabel>
                  3. Подготовим модель {ENGINE_LABEL[settings.engine]}
                </FieldLabel>
                <FieldDescription>
                  Размер: {ENGINE_SIZE[settings.engine]}. Разовая операция — дальше всё работает оффлайн и быстро.
                </FieldDescription>
              </Field>
              {error && (
                <Alert variant="destructive">
                  <AlertDescription className="whitespace-pre-wrap">
                    {error}
                  </AlertDescription>
                </Alert>
              )}
            </FieldGroup>
          )}

          {step === "downloading" && (
            <div className="flex flex-col gap-4">
              <div className="text-sm font-medium">
                {modelStage === "warmup"
                  ? `🔥 Прогреваю модель в памяти… ${progress}%`
                  : `⬇️ Скачиваю модель… ${progress}%`}
              </div>
              <Progress
                value={Math.max(progress, 2)}
                className={modelStage === "warmup" ? "h-2 animate-pulse" : "h-2"}
              />
              <div className="text-xs text-muted-foreground">
                {modelStage === "warmup"
                  ? "Загружаю модель в память. Это разово, обычно занимает 10–30 секунд."
                  : "Загружаю файлы модели."}
              </div>
              {modelStage === "downloading" && modelDownloadDetails(progressDetail) && (
                <div className="text-xs text-muted-foreground">
                  {modelDownloadDetails(progressDetail)}
                </div>
              )}
              {modelStage === "downloading" && isSlowModelDownload(progressDetail) && (
                <div className="text-xs text-muted-foreground">
                  Сервер отдает файл медленно, загрузка продолжается.
                </div>
              )}
            </div>
          )}

          {step === "ready" && (
            <div className="flex flex-col gap-3 text-sm">
              <div>✅ Всё готово!</div>
              <div className="text-muted-foreground">
                Можно начинать расшифровывать аудио и видео.
              </div>
            </div>
          )}
        </CardContent>

        <CardFooter className="justify-end gap-2 bg-white/55 rounded-b-[inherit] px-6 py-4">
          {step === "folder" && settings && (
            <Button onClick={() => setStep("engine")}>Далее</Button>
          )}
          {step === "engine" && settings && (
            <Button onClick={() => setStep("model")}>Далее</Button>
          )}
          {step === "model" && settings && (
            <>
              <Button variant="outline" onClick={() => setStep("engine")}>
                Назад
              </Button>
              <Button
                onClick={downloadModel}
                disabled={selectedEngineStatus?.available === false}
              >
                Подготовить модель
              </Button>
            </>
          )}
          {step === "ready" && (
            <Button onClick={finish}>Начать работу</Button>
          )}
        </CardFooter>
      </Card>
    </main>
  );
}
