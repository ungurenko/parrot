import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
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
import { ENGINE_LABEL, ENGINE_SIZE, type Engine, type Settings } from "../types";
import { EnginePicker } from "./EnginePicker";

interface Props {
  onDone: () => void;
}

type Step = "folder" | "engine" | "model" | "downloading" | "ready";
type ModelStage = "downloading" | "warmup" | "ready";

export function Onboarding({ onDone }: Props) {
  const [step, setStep] = useState<Step>("folder");
  const [settings, setSettings] = useState<Settings | null>(null);
  const [progress, setProgress] = useState(0);
  const [modelStage, setModelStage] = useState<ModelStage>("downloading");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<Settings>("get_settings").then(setSettings);
  }, []);

  useEffect(() => {
    const progressP = listen<number>("model:progress", (e) =>
      setProgress(e.payload),
    );
    const stageP = listen<ModelStage>("model:stage", (e) =>
      setModelStage(e.payload),
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
    await invoke("set_settings", { new: next });
  };

  const downloadModel = async () => {
    setStep("downloading");
    setModelStage("downloading");
    setProgress(0);
    setError(null);
    try {
      await invoke("download_model");
      setStep("ready");
    } catch (e: any) {
      setError(String(e));
      setStep("model");
    }
  };

  const finish = async () => {
    if (settings) {
      await invoke("set_settings", { new: { ...settings, onboarded: true } });
    }
    onDone();
  };

  return (
    <main className="fixed inset-0 flex items-center justify-center overflow-y-auto bg-background p-6">
      <Card className="w-full max-w-lg border bg-card shadow-none">
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-2xl">
            <span aria-hidden="true">🦜</span>
            Parrot
          </CardTitle>
          <CardDescription>
            Настроим приложение перед первым запуском.
          </CardDescription>
        </CardHeader>

        <CardContent>
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
                <EnginePicker value={settings.engine} onChange={changeEngine} />
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

        <CardFooter className="justify-end gap-2 bg-muted/30">
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
              <Button onClick={downloadModel}>Подготовить модель</Button>
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
