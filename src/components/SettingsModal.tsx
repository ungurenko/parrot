import { useEffect, useState, type KeyboardEvent } from "react";
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
  SUMMARIZER_MODEL_LABEL,
  SUMMARIZER_MODEL_SIZE,
  type Engine,
  type EngineStatuses,
  type ModelProgressDetail,
  type Settings,
  type SummarizerStatus,
  type TranscriptLanguage,
} from "../types";
import { EnginePicker } from "./EnginePicker";
import { UpdateChecker } from "./UpdateChecker";
import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/progress";
import { cn } from "@/lib/utils";
import { DownloadIcon, Trash2Icon } from "lucide-react";

import type { AutoUpdate } from "../hooks/useAutoUpdate";

interface Props {
  onClose: () => void;
  updater: AutoUpdate;
}

function normalizeShortcutKey(key: string): string | null {
  const lower = key.toLowerCase();
  if (["meta", "alt", "control", "shift"].includes(lower)) return null;
  if (lower === " ") return "Space";
  if (lower === "escape") return "Escape";
  if (lower === "enter" || lower === "return") return "Enter";
  if (lower === "backspace") return "Backspace";
  if (lower === "tab") return "Tab";
  if (/^f([1-9]|1[0-9]|2[0-4])$/i.test(key)) return key.toUpperCase();
  if (key.length === 1) return key.toUpperCase();
  return key;
}

function isModifierOnlyKey(key: string): boolean {
  return ["meta", "alt", "control", "shift"].includes(key.toLowerCase());
}

function displayShortcut(shortcut: string): string {
  return shortcut
    .split("+")
    .map((part) => (part.trim() === "Alt" ? "Option" : part.trim()))
    .join("+");
}

function displayShortcutParts(shortcut: string): string[] {
  return displayShortcut(shortcut)
    .split("+")
    .map((part) => {
      const trimmed = part.trim();
      if (trimmed === "Command") return "⌘";
      if (trimmed === "Option") return "⌥ Option";
      if (trimmed === "Control") return "⌃ Control";
      if (trimmed === "Shift") return "⇧ Shift";
      return trimmed;
    })
    .filter(Boolean);
}

export function SettingsModal({ onClose, updater }: Props) {
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
  const [summarizerStatus, setSummarizerStatus] =
    useState<SummarizerStatus | null>(null);
  const [summaryBusy, setSummaryBusy] = useState(false);
  const [summaryDeleting, setSummaryDeleting] = useState(false);
  const [summaryProgress, setSummaryProgress] = useState(0);
  const [summaryStage, setSummaryStage] = useState<
    "downloading" | "warmup" | "ready"
  >("downloading");
  const [envSetupBusy, setEnvSetupBusy] = useState(false);
  const [envSetupStatus, setEnvSetupStatus] = useState<string | null>(null);
  const [capturingShortcut, setCapturingShortcut] = useState(false);
  const [shortcutHint, setShortcutHint] = useState<string | null>(null);

  const refreshModelStatuses = () =>
    invoke<EngineStatuses>("get_engine_statuses").then(setModelStatuses);
  const refreshSummarizerStatus = () =>
    invoke<SummarizerStatus>("get_summarizer_status").then(setSummarizerStatus);

  useEffect(() => {
    invoke<Settings>("get_settings").then(setSettings);
    getVersion().then(setAppVersion).catch(() => setAppVersion(""));
    refreshModelStatuses();
    refreshSummarizerStatus();
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
    const summaryProgressP = listen<number>("summary_model:progress", (e) => {
      setSummaryProgress(e.payload);
    });
    const summaryStageP = listen<"downloading" | "warmup" | "ready">(
      "summary_model:stage",
      (e) => setSummaryStage(e.payload),
    );
    const envProgressP = listen<string>("summary_env:progress", (e) =>
      setEnvSetupStatus(e.payload),
    );
    return () => {
      progressP.then((u) => u());
      progressDetailP.then((u) => u());
      stageP.then((u) => u());
      summaryProgressP.then((u) => u());
      summaryStageP.then((u) => u());
      envProgressP.then((u) => u());
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

  const toggleSummarizer = async (enabled: boolean) => {
    if (!settings) return;
    const next = { ...settings, summarizer_enabled: enabled };
    try {
      await invoke("set_settings", { new: next });
      setSettings(next);
      setModelError(null);
    } catch (e: unknown) {
      setModelError(String(e));
    }
  };

  const toggleDictation = async (enabled: boolean) => {
    if (!settings) return;
    const next = { ...settings, dictation_enabled: enabled };
    try {
      await invoke("set_settings", { new: next });
      setSettings(next);
      setModelError(null);
    } catch (e: unknown) {
      setModelError(String(e));
    }
  };

  const changeDictationShortcut = async (shortcut: string) => {
    if (!settings) return;
    const next = {
      ...settings,
      dictation_enabled: true,
      dictation_hold_key: shortcut,
    };
    try {
      await invoke("set_settings", { new: next });
      setSettings(next);
      setModelError(null);
      setShortcutHint(`Сочетание сохранено: ${displayShortcut(shortcut)}`);
    } catch (e: unknown) {
      setModelError(String(e));
      setShortcutHint(null);
    } finally {
      setCapturingShortcut(false);
    }
  };

  const captureDictationShortcut = (e: KeyboardEvent<HTMLButtonElement>) => {
    if (!capturingShortcut) return;
    e.preventDefault();
    e.stopPropagation();

    const key = normalizeShortcutKey(e.key);
    if (!key) {
      if (isModifierOnlyKey(e.key)) {
        setShortcutHint(
          "Один Option пока нельзя выбрать: macOS считает его служебной клавишей. Используйте Option+Space или Command+Shift+Space.",
        );
      } else {
        setShortcutHint("Нажмите обычную клавишу вместе с Command, Option, Control или Shift.");
      }
      return;
    }

    const modifiers = [
      e.metaKey ? "Command" : null,
      e.altKey ? "Alt" : null,
      e.ctrlKey ? "Control" : null,
      e.shiftKey ? "Shift" : null,
    ].filter(Boolean) as string[];

    if (modifiers.length === 0) {
      setShortcutHint("Добавьте Command, Option, Control или Shift.");
      return;
    }

    const shortcut = [...modifiers, key].join("+");
    void changeDictationShortcut(shortcut);
  };

  const setupSummarizerEnv = async () => {
    setEnvSetupBusy(true);
    setModelError(null);
    setEnvSetupStatus("Подготовка…");
    try {
      await invoke("setup_summarizer_env");
      setEnvSetupStatus(null);
      await refreshSummarizerStatus();
    } catch (e: unknown) {
      setModelError(String(e));
      setEnvSetupStatus(null);
    } finally {
      setEnvSetupBusy(false);
    }
  };

  const prepareSummarizerModel = async () => {
    setSummaryBusy(true);
    setModelError(null);
    setSummaryProgress(1);
    setSummaryStage("downloading");
    try {
      await invoke("download_summarizer_model");
      setSummaryProgress(100);
      await refreshSummarizerStatus();
      if (settings && !settings.summarizer_promo_seen) {
        const next = { ...settings, summarizer_promo_seen: true };
        try {
          await invoke("set_settings", { new: next });
          setSettings(next);
        } catch (e) {
          console.error("set_settings (promo_seen) failed:", e);
        }
      }
    } catch (e: unknown) {
      setModelError(String(e));
    } finally {
      setSummaryBusy(false);
    }
  };

  const deleteSummarizerModel = async () => {
    if (!summarizerStatus?.modelReady) return;
    const ok = window.confirm(
      `Удалить модель «${SUMMARIZER_MODEL_LABEL}»?\n\nКонспекты не сохранятся, но транскрипции останутся на месте. При необходимости модель можно скачать заново.`,
    );
    if (!ok) return;
    setSummaryDeleting(true);
    setModelError(null);
    try {
      await invoke("delete_summarizer_model");
      await refreshSummarizerStatus();
      setSummaryProgress(0);
    } catch (e: unknown) {
      setModelError(String(e));
    } finally {
      setSummaryDeleting(false);
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
            <FieldLabel>Диктовка</FieldLabel>
            <div className="dictation-card">
              <div className="dictation-shortcut-row">
                <div className="min-w-0">
                  <div className="dictation-eyebrow">Клавиши для записи</div>
                  <div
                    className="dictation-key-row"
                    aria-label={`Текущее сочетание: ${displayShortcut(settings.dictation_hold_key)}`}
                  >
                    {displayShortcutParts(settings.dictation_hold_key).map((part) => (
                      <span className="dictation-key" key={part}>
                        {part}
                      </span>
                    ))}
                  </div>
                </div>
                <Button
                  type="button"
                  variant={capturingShortcut ? "default" : "outline"}
                  className="dictation-record-button"
                  onClick={() => {
                    setCapturingShortcut(true);
                    setShortcutHint("Нажмите новое сочетание клавиш…");
                  }}
                  onKeyDown={captureDictationShortcut}
                  onBlur={() => {
                    if (capturingShortcut) setCapturingShortcut(false);
                  }}
                >
                  {capturingShortcut ? "Жду сочетание…" : "Записать"}
                </Button>
              </div>

              <div className="dictation-hint" aria-live="polite">
                {shortcutHint ?? "Например: Option+Space или Command+Shift+Space"}
              </div>

              <ol className="dictation-steps">
                <li>
                  <span className="dictation-step-number">1</span>
                  <span>
                    <strong>Зажмите</strong> клавиши, когда хотите продиктовать
                    текст.
                  </span>
                </li>
                <li>
                  <span className="dictation-step-number">2</span>
                  <span>
                    <strong>Скажите фразу</strong> и отпустите клавиши.
                  </span>
                </li>
                <li>
                  <span className="dictation-step-number">3</span>
                  <span>
                    <strong>Вставьте текст</strong> в любое место через ⌘ V.
                  </span>
                </li>
              </ol>

              <label className="dictation-enabled-row">
                <span>
                  <span className="dictation-enabled-title">Функция включена</span>
                  <span className="dictation-enabled-hint">
                    Можно временно отключить, если сочетание мешает.
                  </span>
                </span>
                <input
                  type="checkbox"
                  checked={settings.dictation_enabled}
                  onChange={(e) => toggleDictation(e.target.checked)}
                />
              </label>
            </div>
          </Field>

          <Field className="min-w-0">
            <FieldLabel>🪶 Конспект</FieldLabel>
            <label className="summary-toggle">
              <input
                type="checkbox"
                checked={settings.summarizer_enabled}
                onChange={(e) => toggleSummarizer(e.target.checked)}
              />
              <span>
                Показывать блок конспекта в результате транскрипции
                <span className="summary-toggle-hint">
                  Кнопка «Сгенерировать» появится под каждым готовым
                  транскриптом. Модель Qwen 3-4B Instruct (4-bit MLX), работает
                  полностью оффлайн.
                </span>
              </span>
            </label>

            {settings.summarizer_enabled && (
              <div
                className={cn(
                  "engine-card flex min-w-0 flex-col gap-2",
                  summarizerStatus?.modelReady && "selected",
                  summarizerStatus && !summarizerStatus.available && "unavailable",
                )}
              >
                <div className="flex min-w-0 flex-col gap-3 sm:flex-row sm:items-start">
                  <div className="flex min-w-0 flex-1 items-start gap-3">
                    <span className="min-w-0">
                      <span className="flex min-w-0 flex-wrap items-center gap-2">
                        <span className="font-medium leading-snug">
                          {SUMMARIZER_MODEL_LABEL}
                        </span>
                        <Badge variant="outline">{SUMMARIZER_MODEL_SIZE}</Badge>
                        <Badge variant="secondary">4-bit MLX</Badge>
                        {summarizerStatus && !summarizerStatus.available && (
                          <Badge variant="secondary">Недоступна</Badge>
                        )}
                      </span>
                      <span className="mt-1 block whitespace-normal break-words text-xs leading-relaxed text-muted-foreground">
                        {summarizerStatus?.unavailableReason ??
                          "Разовая загрузка ~2.3 ГБ. После этого конспекты создаются оффлайн."}
                      </span>
                    </span>
                  </div>
                  <div className="flex shrink-0 items-center gap-2 self-start sm:self-auto">
                    <span
                      className={cn(
                        "status-chip",
                        summarizerStatus?.modelReady && "ready",
                      )}
                    >
                      <span className="dot" aria-hidden="true" />
                      {summarizerStatus?.modelReady ? "Скачана" : "Не скачана"}
                    </span>
                    {summarizerStatus && !summarizerStatus.available && (
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={envSetupBusy}
                        onClick={setupSummarizerEnv}
                        title="Установить Python-окружение для конспекта"
                      >
                        {envSetupBusy ? "Устанавливаю…" : "Установить окружение"}
                      </Button>
                    )}
                    {!summarizerStatus?.modelReady &&
                      summarizerStatus?.available && (
                        <Button
                          type="button"
                          variant="outline"
                          size={summaryBusy ? "sm" : "icon-sm"}
                          disabled={summaryBusy || summaryDeleting}
                          onClick={prepareSummarizerModel}
                          title="Подготовить модель конспекта"
                        >
                          {summaryBusy ? (
                            `${summaryProgress}%`
                          ) : (
                            <DownloadIcon data-icon="inline-start" />
                          )}
                        </Button>
                      )}
                    {summarizerStatus?.modelReady && (
                      <Button
                        type="button"
                        variant="destructive"
                        size="icon-sm"
                        disabled={summaryBusy || summaryDeleting}
                        onClick={deleteSummarizerModel}
                        title="Удалить модель конспекта"
                      >
                        <Trash2Icon data-icon="inline-start" />
                      </Button>
                    )}
                  </div>
                </div>

                {summaryBusy && (
                  <div className="flex min-w-0 flex-col gap-2 pl-1">
                    <Progress
                      value={Math.max(summaryProgress, 2)}
                      className={summaryStage === "warmup" ? "animate-pulse" : ""}
                    />
                    <div className="text-xs text-muted-foreground">
                      {summaryStage === "warmup"
                        ? `Прогреваю модель… ${summaryProgress}%`
                        : `Скачиваю модель… ${summaryProgress}%`}
                    </div>
                  </div>
                )}

                {summaryDeleting && (
                  <div className="text-xs text-muted-foreground">
                    Удаляю модель…
                  </div>
                )}

                {(envSetupBusy || envSetupStatus) && (
                  <div className="text-xs text-muted-foreground pl-1">
                    {envSetupStatus ?? "Подготовка окружения…"}
                  </div>
                )}
              </div>
            )}
          </Field>

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
            <UpdateChecker updater={updater} />
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
