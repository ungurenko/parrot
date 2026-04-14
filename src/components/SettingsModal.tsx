import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { ENGINE_LABEL, ENGINE_SIZE, isQwenEngine, type Engine, type Settings } from "../types";
import { EnginePicker } from "./EnginePicker";
import { UpdateChecker } from "./UpdateChecker";

interface Props {
  onClose: () => void;
}

export function SettingsModal({ onClose }: Props) {
  const [settings, setSettings] = useState<Settings | null>(null);
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
    <div
      className="fixed inset-0 bg-black/50 flex items-center justify-center z-50"
      onClick={onClose}
    >
      <div
        className="bg-[var(--color-bg)] border border-[var(--color-border)] rounded-xl p-6 w-[520px] max-h-[85vh] overflow-y-auto shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-lg font-semibold">⚙️ Настройки</h2>
          <button
            onClick={onClose}
            className="text-[var(--color-muted)] hover:text-[var(--color-text)] text-xl"
          >
            ✕
          </button>
        </div>

        <div className="space-y-5">
          <div>
            <label className="text-xs text-[var(--color-muted)] uppercase tracking-wide">
              Движок транскрибации
            </label>
            <div className="mt-2">
              <EnginePicker value={settings.engine} onChange={changeEngine} />
            </div>

            <div className="mt-3 p-3 rounded-md bg-[var(--color-panel)] border border-[var(--color-border)]">
              <div className="flex items-center justify-between gap-3">
                <div className="text-sm min-w-0">
                  <div className="font-medium truncate">{engineLabel}</div>
                  <div className="text-xs text-[var(--color-muted)] mt-0.5">
                    {modelReady
                      ? `✅ Модель готова (${engineSize})`
                      : `Нужно подготовить перед первым запуском (${engineSize})`}
                  </div>
                </div>
                {!modelReady && (
                  <button
                    onClick={prepareModel}
                    disabled={modelBusy}
                    className="shrink-0 text-xs px-3 py-2 rounded-md bg-[var(--color-accent)] text-white hover:bg-[var(--color-accent-hover)] disabled:opacity-60"
                  >
                    {modelBusy ? `${modelProgress}%` : prepareLabel}
                  </button>
                )}
              </div>
              {modelBusy && (
                <div className="mt-3">
                  <div className="h-1.5 bg-[var(--color-border)] rounded overflow-hidden">
                    <div
                      className={`h-full bg-[var(--color-accent)] transition-all ${
                        modelStage === "warmup" ? "animate-pulse" : ""
                      }`}
                      style={{ width: `${Math.max(modelProgress, 2)}%` }}
                    />
                  </div>
                  <div className="text-xs text-[var(--color-muted)] mt-1">
                    {modelProgress >= 100
                      ? "✅ Готово"
                      : modelStage === "warmup"
                      ? `🔥 Прогреваю в памяти… ${modelProgress}% (разово, ~10–30 сек)`
                      : `⬇️ Скачиваю модель… ${modelProgress}%`}
                  </div>
                </div>
              )}
              {modelError && (
                <div className="mt-2 text-xs text-red-400 p-3 bg-red-500/10 rounded-md border border-red-500/20">
                  {modelError}
                </div>
              )}
            </div>
          </div>

          <div>
            <label className="text-xs text-[var(--color-muted)] uppercase tracking-wide">
              Папка сохранения
            </label>
            <div className="mt-1 flex items-center gap-2">
              <div className="flex-1 text-sm truncate px-3 py-2 rounded-md bg-[var(--color-panel)] border border-[var(--color-border)]">
                {settings.save_dir}
              </div>
              <button
                onClick={pickFolder}
                className="text-xs px-3 py-2 rounded-md bg-[var(--color-panel)] border border-[var(--color-border)] hover:bg-[var(--color-panel-hover)]"
              >
                Выбрать…
              </button>
            </div>
          </div>

          <div className="flex items-center justify-between gap-4 pt-2 border-t border-[var(--color-border)]">
            <div className="flex flex-col gap-1">
              <button
                onClick={openLogs}
                className="text-left text-xs text-[var(--color-muted)] hover:text-[var(--color-text)]"
              >
                📜 Открыть логи
              </button>
              <UpdateChecker />
            </div>
            <div className="text-right text-xs text-[var(--color-muted)]">
              <div>Разработано Александром Унгуренко, 2026</div>
              <div>v0.1.0</div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
