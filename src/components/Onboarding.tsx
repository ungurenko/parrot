import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import parrotImg from "/parrot.png";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Field,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Progress } from "@/components/ui/progress";
import { modelDownloadDetails } from "@/lib/modelProgress";
import { cleanupTauriListeners, listenInTauri } from "@/lib/runtime";
import { modeOptionForEngine } from "@/lib/engineModes";
import { modelProgressMessage } from "@/lib/progressEstimate";
import { userErrorFrom } from "@/lib/userErrors";
import {
  type Engine,
  type EngineStatuses,
  type ModelProgressDetail,
  type Settings,
} from "../types";
import { EnginePicker } from "./EnginePicker";

interface Props {
  onDone: () => void;
}

type Step = "setup" | "downloading" | "ready";
type ModelStage = "downloading" | "warmup" | "ready";

export function Onboarding({ onDone }: Props) {
  const [step, setStep] = useState<Step>("setup");
  const [settings, setSettings] = useState<Settings | null>(null);
  const [engineStatuses, setEngineStatuses] = useState<EngineStatuses>({});
  const [progress, setProgress] = useState(0);
  const [progressDetail, setProgressDetail] =
    useState<ModelProgressDetail | null>(null);
  const [modelStage, setModelStage] = useState<ModelStage>("downloading");
  const [error, setError] = useState<string | null>(null);
  const [showEngineChoice, setShowEngineChoice] = useState(false);
  const selectedEngineStatus = settings ? engineStatuses[settings.engine] : undefined;
  const selectedMode = settings ? modeOptionForEngine(settings.engine) : null;
  const progressMessage = modelProgressMessage({
    stage: modelStage,
    percent: progress,
    detail: progressDetail,
  });

  useEffect(() => {
    invoke<Settings>("get_settings").then(setSettings);
    invoke<EngineStatuses>("get_engine_statuses").then(setEngineStatuses);
  }, []);

  useEffect(() => {
    const listeners = [
      listenInTauri<number>("model:progress", (e) => setProgress(e.payload)),
      listenInTauri<ModelProgressDetail>("model:progress_detail", (e) =>
        setProgressDetail(e.payload),
      ),
      listenInTauri<ModelStage>("model:stage", (e) =>
        setModelStage(e.payload),
      ),
    ];
    return () => {
      cleanupTauriListeners(listeners);
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

  const chooseFastMode = async () => {
    await changeEngine("parakeet");
    setShowEngineChoice(false);
  };

  const prepareOrFinish = async () => {
    if (selectedEngineStatus?.modelReady) {
      await finish();
      return;
    }
    await downloadModel();
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
      const friendly = userErrorFrom(e);
      setError(`${friendly.message}\n${friendly.action}`);
      setStep("setup");
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
          <p className="text-sm text-muted-foreground">
            Подготовим Parrot к первой расшифровке. Без лишних настроек.
          </p>
        </CardHeader>

        <CardContent className="px-6 pb-6">
          {error && (
            <Alert variant="destructive" className="mb-4">
              <AlertDescription className="whitespace-pre-wrap">
                {error}
              </AlertDescription>
            </Alert>
          )}

          {step === "setup" && settings && selectedMode && (
            <FieldGroup className="gap-5">
              <Field>
                <FieldLabel htmlFor="onboarding-save-dir">
                  Папка для готовых текстов
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

              <div className="quick-start-card">
                <div className="quick-start-copy">
                  <div className="quick-start-eyebrow">
                    Режим распознавания
                  </div>
                  <div className="quick-start-title">{selectedMode.title}</div>
                  <div className="quick-start-text">{selectedMode.detail}</div>
                  <div className="quick-start-tech">
                    Модель: {selectedMode.technicalName} · {selectedMode.size}
                  </div>
                </div>
                <Button
                  type="button"
                  variant="outline"
                  onClick={() => setShowEngineChoice((value) => !value)}
                >
                  {showEngineChoice ? "Скрыть режимы" : "Выбрать другой режим"}
                </Button>
              </div>

              {selectedEngineStatus?.available === false && (
                <Alert>
                  <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                    <span>
                      Этот режим сейчас недоступен. Быстрый режим можно подготовить сразу.
                    </span>
                    <Button type="button" variant="outline" onClick={chooseFastMode}>
                      Выбрать быстрый режим
                    </Button>
                  </AlertDescription>
                </Alert>
              )}

              {showEngineChoice && (
                <Field>
                  <FieldLabel>Выберите режим под вашу запись</FieldLabel>
                  <EnginePicker
                    value={settings.engine}
                    statuses={engineStatuses}
                    onChange={changeEngine}
                  />
                </Field>
              )}

              {!selectedEngineStatus?.modelReady && (
                <div className="text-xs leading-relaxed text-muted-foreground">
                  Модель нужно скачать один раз. После этого распознавание работает локально на Mac.
                </div>
              )}
            </FieldGroup>
          )}

          {step === "downloading" && (
            <div className="flex flex-col gap-4">
              <div className="text-sm font-medium">{progressMessage.title}</div>
              <Progress
                value={Math.max(progress, 2)}
                className={modelStage === "warmup" ? "h-2 animate-pulse" : "h-2"}
              />
              <div className="text-xs text-muted-foreground">{progressMessage.detail}</div>
              {modelStage === "downloading" && modelDownloadDetails(progressDetail) && (
                <div className="text-xs text-muted-foreground">
                  {modelDownloadDetails(progressDetail)}
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

        <CardFooter className="justify-end gap-2 rounded-b-[inherit] bg-white/55 px-6 py-4">
          {step === "setup" && settings && (
            <Button
              onClick={prepareOrFinish}
              disabled={selectedEngineStatus?.available === false}
            >
              {selectedEngineStatus?.modelReady
                ? "Начать работу"
                : "Подготовить Parrot и начать"}
            </Button>
          )}
          {step === "ready" && (
            <Button onClick={finish}>Начать работу</Button>
          )}
        </CardFooter>
      </Card>
    </main>
  );
}
