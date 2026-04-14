import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
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
    <div className="fixed inset-0 bg-[var(--color-bg)] flex items-center justify-center z-50 overflow-y-auto">
      <div className="max-w-lg w-full p-8">
        <h1 className="text-2xl font-semibold mb-2">🦜 Parrot</h1>
        <p className="text-sm text-[var(--color-muted)] mb-6">
          Настроим приложение перед первым запуском
        </p>

        {step === "folder" && settings && (
          <div className="space-y-4">
            <div>
              <div className="text-sm mb-2">
                1. Выберите папку для транскрипций:
              </div>
              <div className="flex items-center gap-2">
                <div className="flex-1 text-sm truncate px-3 py-2 rounded-md bg-[var(--color-panel)] border border-[var(--color-border)]">
                  {settings.save_dir}
                </div>
                <button
                  onClick={pickFolder}
                  className="text-xs px-3 py-2 rounded-md bg-[var(--color-panel)] border border-[var(--color-border)] hover:bg-[var(--color-panel-hover)]"
                >
                  Изменить…
                </button>
              </div>
            </div>
            <button
              onClick={() => setStep("engine")}
              className="w-full py-2 rounded-md bg-[var(--color-accent)] text-white font-medium hover:bg-[var(--color-accent-hover)]"
            >
              Далее
            </button>
          </div>
        )}

        {step === "engine" && settings && (
          <div className="space-y-4">
            <div className="text-sm">
              2. Выберите движок транскрибации. Можно будет поменять в настройках.
            </div>
            <EnginePicker value={settings.engine} onChange={changeEngine} />
            <button
              onClick={() => setStep("model")}
              className="w-full py-2 rounded-md bg-[var(--color-accent)] text-white font-medium hover:bg-[var(--color-accent-hover)]"
            >
              Далее
            </button>
          </div>
        )}

        {step === "model" && settings && (
          <div className="space-y-4">
            <div className="text-sm">
              3. Подготовим модель {ENGINE_LABEL[settings.engine]} ({ENGINE_SIZE[settings.engine]}).
              Разовая операция — дальше всё работает оффлайн и быстро.
            </div>
            {error && (
              <div className="text-xs text-red-400 p-3 bg-red-500/10 rounded-md border border-red-500/20">
                {error}
              </div>
            )}
            <div className="flex gap-2">
              <button
                onClick={() => setStep("engine")}
                className="px-4 py-2 rounded-md bg-[var(--color-panel)] border border-[var(--color-border)] hover:bg-[var(--color-panel-hover)] text-sm"
              >
                Назад
              </button>
              <button
                onClick={downloadModel}
                className="flex-1 py-2 rounded-md bg-[var(--color-accent)] text-white font-medium hover:bg-[var(--color-accent-hover)]"
              >
                Подготовить модель
              </button>
            </div>
          </div>
        )}

        {step === "downloading" && (
          <div className="space-y-4">
            <div className="text-sm">
              {modelStage === "warmup"
                ? `🔥 Прогреваю модель в памяти… ${progress}%`
                : `⬇️ Скачиваю модель… ${progress}%`}
            </div>
            <div className="h-2 bg-[var(--color-panel)] rounded overflow-hidden">
              <div
                className={`h-full bg-[var(--color-accent)] transition-all ${
                  modelStage === "warmup" ? "animate-pulse" : ""
                }`}
                style={{ width: `${Math.max(progress, 2)}%` }}
              />
            </div>
            <div className="text-xs text-[var(--color-muted)]">
              {modelStage === "warmup"
                ? "Загружаю веса в RAM и проверяю инференс. Это разово, ~10–30 сек."
                : "Загружаю файлы модели с HuggingFace."}
            </div>
          </div>
        )}

        {step === "ready" && (
          <div className="space-y-4">
            <div className="text-sm">✅ Всё готово!</div>
            <button
              onClick={finish}
              className="w-full py-2 rounded-md bg-[var(--color-accent)] text-white font-medium hover:bg-[var(--color-accent-hover)]"
            >
              Начать работу
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
