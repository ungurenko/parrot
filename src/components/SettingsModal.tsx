import {
  useEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react";
import { toast } from "sonner";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
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
  ACTIVE_JOB_DELETE_HINT,
  ACTIVE_JOB_SWITCH_HINT,
  DEFAULT_SUMMARY_MODEL,
  ENGINE_LABEL,
  LANGUAGE_LABEL,
  SUMMARY_MODEL_BADGE,
  SUMMARY_MODEL_LABEL,
  SUMMARY_MODEL_SIZE,
  THEME_LABEL,
  type Engine,
  type EngineStatuses,
  type ModelProgressDetail,
  type ModelStage,
  type Settings,
  type SummaryModel,
  type SummarizerStatus,
  type Theme,
  type TranscriptLanguage,
} from "../types";
import { EnginePicker } from "./EnginePicker";
import { UpdateChecker } from "./UpdateChecker";
import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/progress";
import { cn } from "@/lib/utils";
import {
  cleanupTauriListeners,
  isTauriRuntime,
  listenInTauri,
  previewSettings,
} from "@/lib/runtime";
import { formatErrorDescription, isCancelledError } from "@/lib/userErrors";
import { displayShortcut } from "@/lib/shortcuts";
import {
  DatabaseIcon,
  DownloadIcon,
  FileTextIcon,
  KeyboardIcon,
  MonitorIcon,
  MoonIcon,
  RefreshCwIcon,
  SettingsIcon,
  SunIcon,
  Trash2Icon,
  type LucideIcon,
} from "lucide-react";

import type { AutoUpdate } from "../hooks/useAutoUpdate";

interface Props {
  onClose: () => void;
  updater: AutoUpdate;
  hasActiveJob: boolean;
  initialTab?: SettingsTab;
}


type ShortcutKeyboardEvent = {
  key: string;
  code?: string;
  metaKey: boolean;
  altKey: boolean;
  ctrlKey: boolean;
  shiftKey: boolean;
  preventDefault: () => void;
  stopPropagation: () => void;
};

type AccessibilityPermissionState = "checking" | "request" | "verify" | "granted";
export type SettingsTab = "basic" | "models" | "dictation" | "summary" | "updates";

const ACCESSIBILITY_SETTINGS_URL =
  "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

const PREVIEW_ENGINE_STATUSES: EngineStatuses = {
  parakeet: { available: true, modelReady: true },
  whisper: { available: true, modelReady: false },
  "qwen-0.6b": {
    available: false,
    modelReady: false,
    unavailableReason: "В предпросмотре Qwen ASR недоступен.",
  },
  "qwen-1.7b": {
    available: false,
    modelReady: false,
    unavailableReason: "В предпросмотре Qwen ASR недоступен.",
  },
};

const SETTINGS_TABS: Array<{
  id: SettingsTab;
  label: string;
  hint: string;
  Icon: LucideIcon;
}> = [
  {
    id: "basic",
    label: "Основное",
    hint: "Папка и язык",
    Icon: SettingsIcon,
  },
  {
    id: "models",
    label: "Модели",
    hint: "Распознавание",
    Icon: DatabaseIcon,
  },
  {
    id: "dictation",
    label: "Диктовка",
    hint: "Клавиши и доступ",
    Icon: KeyboardIcon,
  },
  {
    id: "summary",
    label: "Конспект",
    hint: "Локальный LLM",
    Icon: FileTextIcon,
  },
  {
    id: "updates",
    label: "Обновления",
    hint: "Версия и логи",
    Icon: RefreshCwIcon,
  },
];

interface ModelSettingsSectionProps {
  settings: Settings;
  modelStatuses: EngineStatuses;
  busyEngine: Engine | null;
  failedEngine: Engine | null;
  deletingEngine: Engine | null;
  modelProgress: number;
  modelProgressDetail: ModelProgressDetail | null;
  modelStage: ModelStage;
  hasActiveJob: boolean;
  onEngineChange: (engine: Engine) => void;
  onPrepareModel: (engine: Engine) => void;
  onDeleteModel: (engine: Engine) => void;
}

function ModelSettingsSection({
  settings,
  modelStatuses,
  busyEngine,
  failedEngine,
  deletingEngine,
  modelProgress,
  modelProgressDetail,
  modelStage,
  hasActiveJob,
  onEngineChange,
  onPrepareModel,
  onDeleteModel,
}: ModelSettingsSectionProps) {
  return (
    <>
      <Field className="min-w-0">
      <FieldLabel>Режим распознавания</FieldLabel>
        <EnginePicker
          value={settings.engine}
          statuses={modelStatuses}
          busyEngine={busyEngine}
          failedEngine={failedEngine}
          deletingEngine={deletingEngine}
          progress={modelProgress}
          progressDetail={modelProgressDetail}
          stage={modelStage}
          hasActiveJob={hasActiveJob}
          onChange={onEngineChange}
          onPrepare={onPrepareModel}
          onDelete={onDeleteModel}
        />
      </Field>

    </>
  );
}

interface SettingsTabsProps {
  active: SettingsTab;
  onChange: (tab: SettingsTab) => void;
}

function SettingsTabs({ active, onChange }: SettingsTabsProps) {
  return (
    <nav className="settings-nav" role="tablist" aria-label="Разделы настроек">
      {SETTINGS_TABS.map(({ Icon, ...tab }) => {
        const selected = active === tab.id;
        return (
          <button
            key={tab.id}
            type="button"
            role="tab"
            aria-selected={selected}
            autoFocus={selected}
            className={cn("settings-nav-item", selected && "active")}
            onClick={() => onChange(tab.id)}
          >
            <Icon className="settings-nav-icon" aria-hidden="true" />
            <span className="settings-nav-copy">
              <span className="settings-nav-label">{tab.label}</span>
              <span className="settings-nav-hint">{tab.hint}</span>
            </span>
          </button>
        );
      })}
    </nav>
  );
}

interface LanguageSettingsSectionProps {
  language: TranscriptLanguage;
  onLanguageChange: (language: TranscriptLanguage) => void;
}

function LanguageSettingsSection({
  language,
  onLanguageChange,
}: LanguageSettingsSectionProps) {
  return (
    <Field className="min-w-0">
      <FieldLabel htmlFor="language">Язык аудио</FieldLabel>
      <select
        id="language"
        value={language}
        onChange={(e) => onLanguageChange(e.target.value as TranscriptLanguage)}
        className="glass-select"
      >
        {Object.entries(LANGUAGE_LABEL).map(([value, label]) => (
          <option key={value} value={value}>
            {label}
          </option>
        ))}
      </select>
    </Field>
  );
}

interface DictationSettingsSectionProps {
  settings: Settings;
  capturingShortcut: boolean;
  shortcutHint: string | null;
  accessibilityPermission: AccessibilityPermissionState;
  onStartCapture: () => void;
  onCaptureShortcut: (
    e: ShortcutKeyboardEvent | ReactKeyboardEvent<HTMLButtonElement>,
  ) => void;
  onStopCapture: () => void;
  onVerifyAccessibility: () => void;
  onRequestAccessibility: () => void;
  onToggleDictation: (enabled: boolean) => void;
}

function DictationSettingsSection({
  settings,
  capturingShortcut,
  shortcutHint,
  accessibilityPermission,
  onStartCapture,
  onCaptureShortcut,
  onStopCapture,
  onVerifyAccessibility,
  onRequestAccessibility,
  onToggleDictation,
}: DictationSettingsSectionProps) {
  return (
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
            onClick={onStartCapture}
            onKeyDown={onCaptureShortcut}
            onBlur={() => {
              if (capturingShortcut) onStopCapture();
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
              <strong>Зажмите</strong> клавиши, когда хотите продиктовать текст.
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
              <strong>Текст вставится</strong> в активное поле автоматически.
            </span>
          </li>
        </ol>

        {accessibilityPermission !== "granted" && (
          <div className="dictation-permission">
            <div className="min-w-0">
              <div className="dictation-permission-title">
                Нужно разрешение macOS
              </div>
              <div className="dictation-permission-hint">
                Без него Parrot сможет скопировать текст, но не вставит его
                автоматически. Если Parrot уже включён в списке, выключите и
                включите его снова. Если ошибка останется, удалите Parrot из
                списка и добавьте заново.
              </div>
            </div>
            <Button
              type="button"
              variant="outline"
              className="dictation-permission-button"
              onClick={
                accessibilityPermission === "verify"
                  ? onVerifyAccessibility
                  : onRequestAccessibility
              }
              disabled={accessibilityPermission === "checking"}
            >
              {accessibilityPermission === "checking"
                ? "Проверяю…"
                : accessibilityPermission === "verify"
                  ? "Проверить"
                  : "Разрешить"}
            </Button>
          </div>
        )}

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
            onChange={(e) => onToggleDictation(e.target.checked)}
          />
        </label>
      </div>
    </Field>
  );
}

interface AppearanceSectionProps {
  theme: Theme;
  onThemeChange: (theme: Theme) => void;
}

const THEME_OPTIONS: Array<{
  id: Theme;
  label: string;
  Icon: LucideIcon;
}> = [
  { id: "system", label: THEME_LABEL.system, Icon: MonitorIcon },
  { id: "light", label: THEME_LABEL.light, Icon: SunIcon },
  { id: "dark", label: THEME_LABEL.dark, Icon: MoonIcon },
];

function AppearanceSection({ theme, onThemeChange }: AppearanceSectionProps) {
  return (
    <Field className="min-w-0">
      <FieldLabel>Оформление</FieldLabel>
      <div className="grid gap-2 sm:grid-cols-3">
        {THEME_OPTIONS.map(({ Icon, ...opt }) => {
          const selected = theme === opt.id;
          return (
            <button
              key={opt.id}
              type="button"
              className={cn(
                "engine-card min-w-0 text-left",
                selected && "selected",
              )}
              onClick={() => onThemeChange(opt.id)}
            >
              <span className="flex min-w-0 items-center gap-2">
                <Icon className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
                <span className="font-medium leading-snug">{opt.label}</span>
              </span>
            </button>
          );
        })}
      </div>
    </Field>
  );
}

interface SaveFolderSectionProps {
  saveDir: string;
  onPickFolder: () => void;
}

function SaveFolderSection({ saveDir, onPickFolder }: SaveFolderSectionProps) {
  return (
    <Field className="min-w-0">
      <FieldLabel htmlFor="save-dir">Папка сохранения</FieldLabel>
      <div className="flex min-w-0 items-center gap-2">
        <Input
          id="save-dir"
          readOnly
          value={saveDir}
          className="glass-input min-w-0 truncate"
        />
        <Button
          type="button"
          variant="outline"
          onClick={onPickFolder}
          className="shrink-0"
        >
          Выбрать…
        </Button>
      </div>
    </Field>
  );
}

interface UpdatesSettingsSectionProps {
  updater: AutoUpdate;
  onOpenLogs: () => void;
}

function UpdatesSettingsSection({
  updater,
  onOpenLogs,
}: UpdatesSettingsSectionProps) {
  return (
    <Field className="min-w-0">
      <FieldLabel>Обновления и диагностика</FieldLabel>
      <div className="settings-simple-card">
        <div className="settings-simple-copy">
          <div className="settings-simple-title">Версия приложения</div>
          <div className="settings-simple-text">
            Проверка обновлений и логи для диагностики ошибок.
          </div>
        </div>
        <UpdateChecker updater={updater} />
        <Button
          type="button"
          variant="outline"
          className="self-start"
          onClick={onOpenLogs}
        >
          Открыть логи
        </Button>
      </div>
    </Field>
  );
}

interface SummarySettingsSectionProps {
  enabled: boolean;
  summaryModel: SummaryModel;
  status: SummarizerStatus | null;
  busy: boolean;
  deleting: boolean;
  progress: number;
  stage: ModelStage;
  envSetupBusy: boolean;
  envSetupStatus: string | null;
  onToggle: (enabled: boolean) => void;
  onSummaryModelChange: (model: SummaryModel) => void;
  onSetupEnv: () => void;
  onPrepareModel: () => void;
  onDeleteModel: () => void;
}

function SummarySettingsSection({
  enabled,
  summaryModel,
  status,
  busy,
  deleting,
  progress,
  stage,
  envSetupBusy,
  envSetupStatus,
  onToggle,
  onSummaryModelChange,
  onSetupEnv,
  onPrepareModel,
  onDeleteModel,
}: SummarySettingsSectionProps) {
  return (
    <Field className="min-w-0">
      <FieldLabel>Конспект</FieldLabel>
      <label className="summary-toggle">
        <input
          type="checkbox"
          checked={enabled}
          onChange={(e) => onToggle(e.target.checked)}
        />
        <span>
          Показывать блок конспекта в результате транскрипции
          <span className="summary-toggle-hint">
            Кнопка «Сгенерировать» появится под каждым готовым транскриптом.
            Выбранная модель конспекта работает полностью оффлайн после первой
            загрузки.
          </span>
        </span>
      </label>

      {enabled && (
        <div className="flex min-w-0 flex-col gap-2">
          <div className="grid gap-2 sm:grid-cols-2">
            {(["qwen3-4b", "gemma4-e2b"] as SummaryModel[]).map((model) => (
              <button
                key={model}
                type="button"
                className={cn(
                  "engine-card min-w-0 text-left",
                  summaryModel === model && "selected",
                )}
                onClick={() => onSummaryModelChange(model)}
                disabled={busy || deleting}
              >
                <span className="flex min-w-0 flex-wrap items-center gap-2">
                  <span className="font-medium leading-snug">
                    {SUMMARY_MODEL_LABEL[model]}
                  </span>
                  <Badge variant="outline">{SUMMARY_MODEL_SIZE[model]}</Badge>
                  <Badge variant={model === "gemma4-e2b" ? "secondary" : "outline"}>
                    {SUMMARY_MODEL_BADGE[model]}
                  </Badge>
                </span>
                <span className="mt-1 block text-xs leading-relaxed text-muted-foreground">
                  {model === "qwen3-4b"
                    ? "Занимает меньше места и остаётся запасным вариантом."
                    : "Рекомендуется: быстрее создаёт подробные русские конспекты."}
                </span>
              </button>
            ))}
          </div>

          <div
            className={cn(
              "engine-card flex min-w-0 flex-col gap-2",
              status?.modelReady && "selected",
              status && !status.available && "unavailable",
            )}
          >
            <div className="flex min-w-0 flex-col gap-3 sm:flex-row sm:items-start">
              <div className="flex min-w-0 flex-1 items-start gap-3">
                <span className="min-w-0">
                  <span className="flex min-w-0 flex-wrap items-center gap-2">
                    <span className="font-medium leading-snug">
                      {SUMMARY_MODEL_LABEL[summaryModel]}
                    </span>
                    <Badge variant="outline">{SUMMARY_MODEL_SIZE[summaryModel]}</Badge>
                    <Badge variant="secondary">4-bit MLX</Badge>
                    {status && !status.available && (
                      <Badge variant="secondary">Недоступна</Badge>
                    )}
                  </span>
                  <span className="mt-1 block whitespace-normal break-words text-xs leading-relaxed text-muted-foreground">
                    {status?.unavailableReason
                      ? formatErrorDescription(status.unavailableReason)
                      : `Разовая загрузка ${SUMMARY_MODEL_SIZE[summaryModel]}. После этого конспекты создаются оффлайн.`}
                  </span>
                </span>
              </div>
              <div className="flex shrink-0 items-center gap-2 self-start sm:self-auto">
                <span className={cn("status-chip", status?.modelReady && "ready")}>
                  <span className="dot" aria-hidden="true" />
                  {status?.modelReady ? "Скачана" : "Не скачана"}
                </span>
                {status && !status.available && (
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={envSetupBusy}
                    onClick={onSetupEnv}
                    title="Установить Python-окружение для конспекта"
                  >
                    {envSetupBusy ? "Устанавливаю…" : "Установить окружение"}
                  </Button>
                )}
                {!status?.modelReady && status?.available && (
                  <Button
                    type="button"
                    variant="outline"
                    size={busy ? "sm" : "icon-sm"}
                    disabled={busy || deleting}
                    onClick={onPrepareModel}
                    title="Подготовить модель конспекта"
                  >
                    {busy ? `${progress}%` : <DownloadIcon data-icon="inline-start" />}
                  </Button>
                )}
                {status?.modelReady && (
                  <Button
                    type="button"
                    variant="destructive"
                    size="icon-sm"
                    disabled={busy || deleting}
                    onClick={onDeleteModel}
                    title="Удалить модель конспекта"
                  >
                    <Trash2Icon data-icon="inline-start" />
                  </Button>
                )}
              </div>
            </div>

            {busy && (
              <div className="flex min-w-0 flex-col gap-2 pl-1">
                <Progress
                  value={Math.max(progress, 2)}
                  className={stage === "warmup" ? "animate-pulse" : ""}
                />
                <div className="text-xs text-muted-foreground">
                  {stage === "warmup"
                    ? `Прогреваю модель… ${progress}%`
                    : `Скачиваю модель ${SUMMARY_MODEL_SIZE[summaryModel]}… ${progress}%`}
                </div>
              </div>
            )}

            {deleting && (
              <div className="text-xs text-muted-foreground">Удаляю модель…</div>
            )}

            {(envSetupBusy || envSetupStatus) && (
              <div className="text-xs text-muted-foreground pl-1">
                {envSetupStatus ?? "Подготовка окружения…"}
              </div>
            )}
          </div>
        </div>
      )}
    </Field>
  );
}

interface SettingsFooterProps {
  appVersion: string;
}

function SettingsFooter({ appVersion }: SettingsFooterProps) {
  return (
    <>
      <Separator className="bg-hairline" />

      <div className="modal-band-bottom flex min-w-0 shrink-0 items-center justify-between gap-4 p-4">
        <div className="min-w-0 text-left text-xs text-muted-foreground">
          <div>Разработано Александром Унгуренко, 2026</div>
          {appVersion && <div>v{appVersion}</div>}
        </div>
      </div>
    </>
  );
}

function normalizeShortcutKey(key: string, code?: string): string | null {
  const lower = key.toLowerCase();
  if (["meta", "alt", "control", "shift"].includes(lower)) return null;
  if (code?.startsWith("Key") && code.length === 4) return code.slice(3);
  if (code?.startsWith("Digit") && code.length === 6) return code.slice(5);
  if (code === "Space") return "Space";
  if (code === "Escape") return "Escape";
  if (code === "Enter" || code === "Return") return "Enter";
  if (code === "Backspace") return "Backspace";
  if (code === "Tab") return "Tab";
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

export function SettingsModal({
  onClose,
  updater,
  hasActiveJob,
  initialTab,
}: Props) {
  const previewMode = !isTauriRuntime();
  const [settings, setSettings] = useState<Settings | null>(null);
  const [activeTab, setActiveTab] = useState<SettingsTab>(initialTab ?? "basic");
  const [appVersion, setAppVersion] = useState("");
  const [modelStatuses, setModelStatuses] = useState<EngineStatuses>({});
  const [busyEngine, setBusyEngine] = useState<Engine | null>(null);
  const [failedEngine, setFailedEngine] = useState<Engine | null>(null);
  const [deletingEngine, setDeletingEngine] = useState<Engine | null>(null);
  const [settingsError, setSettingsError] = useState<string | null>(null);
  const [modelProgress, setModelProgress] = useState(0);
  const [modelProgressDetail, setModelProgressDetail] =
    useState<ModelProgressDetail | null>(null);
  const [modelStage, setModelStage] = useState<ModelStage>("downloading");
  const [summarizerStatus, setSummarizerStatus] =
    useState<SummarizerStatus | null>(null);
  const [summaryBusy, setSummaryBusy] = useState(false);
  const [summaryDeleting, setSummaryDeleting] = useState(false);
  const [summaryProgress, setSummaryProgress] = useState(0);
  const [summaryStage, setSummaryStage] = useState<ModelStage>("downloading");
  const [envSetupBusy, setEnvSetupBusy] = useState(false);
  const [envSetupStatus, setEnvSetupStatus] = useState<string | null>(null);
  const [capturingShortcut, setCapturingShortcut] = useState(false);
  const [shortcutHint, setShortcutHint] = useState<string | null>(null);
  const [accessibilityPermission, setAccessibilityPermission] =
    useState<AccessibilityPermissionState>("checking");
  const settingsRef = useRef<Settings | null>(null);

  useEffect(() => {
    settingsRef.current = settings;
  }, [settings]);

  const refreshModelStatuses = async (): Promise<EngineStatuses> => {
    if (!isTauriRuntime()) {
      setModelStatuses(PREVIEW_ENGINE_STATUSES);
      return PREVIEW_ENGINE_STATUSES;
    }
    const nextStatuses = await invoke<EngineStatuses>("get_engine_statuses");
    setModelStatuses(nextStatuses);
    return nextStatuses;
  };
  const refreshSummarizerStatus = () => {
    if (!isTauriRuntime()) {
      setSummarizerStatus({ available: true, modelReady: false });
      return Promise.resolve();
    }
    return invoke<SummarizerStatus>("get_summarizer_status").then(
      setSummarizerStatus,
    );
  };

  useEffect(() => {
    if (!isTauriRuntime()) {
      setSettings(previewSettings("~/Documents/Parrot"));
      setAppVersion("preview");
      setModelStatuses(PREVIEW_ENGINE_STATUSES);
      setSummarizerStatus({ available: true, modelReady: false });
      setAccessibilityPermission("request");
      return;
    }
    invoke<Settings>("get_settings").then(setSettings);
    getVersion().then(setAppVersion).catch(() => setAppVersion(""));
    refreshModelStatuses();
    refreshSummarizerStatus();
    invoke<boolean>("check_accessibility_permission")
      .then((granted) => setAccessibilityPermission(granted ? "granted" : "request"))
      .catch(() => setAccessibilityPermission("request"));
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    const listeners = [
      listenInTauri<number>("model:progress", (e) => {
        setModelProgress(e.payload);
      }),
      listenInTauri<ModelProgressDetail>("model:progress_detail", (e) => {
        setModelProgressDetail(e.payload);
      }),
      listenInTauri<ModelStage>("model:stage", (e) =>
        setModelStage(e.payload),
      ),
      listenInTauri<number>("summary_model:progress", (e) => {
        setSummaryProgress(e.payload);
      }),
      listenInTauri<ModelStage>(
        "summary_model:stage",
        (e) => setSummaryStage(e.payload),
      ),
      listenInTauri<string>("summary_env:progress", (e) =>
        setEnvSetupStatus(e.payload),
      ),
    ];
    return () => {
      cleanupTauriListeners(listeners);
    };
  }, []);

  const saveSettings = async (next: Settings): Promise<boolean> => {
    if (previewMode) {
      setSettings(next);
      settingsRef.current = next;
      setSettingsError(null);
      return true;
    }
    try {
      await invoke("set_settings", { new: next });
      setSettings(next);
      settingsRef.current = next;
      setSettingsError(null);
      return true;
    } catch (e: unknown) {
      setSettingsError(formatErrorDescription(e));
      return false;
    }
  };

  const pickFolder = async () => {
    if (previewMode) return;
    const result = await open({ directory: true, multiple: false });
    if (!result || !settings) return;
    const next = { ...settings, save_dir: result as string };
    if (await saveSettings(next)) toast.success("Сохранено");
  };

  const changeEngine = async (engine: Engine) => {
    if (!settings) return;
    if (engine === settings.engine) return;
    if (engine !== settings.engine && hasActiveJob) {
      setSettingsError(ACTIVE_JOB_SWITCH_HINT);
      return;
    }
    if (!modelStatuses[engine]?.modelReady) {
      setSettingsError(
        "Сначала скачайте модель. После загрузки Parrot выберет её автоматически.",
      );
      return;
    }
    const next = { ...settings, engine };
    if (await saveSettings(next)) toast.success("Сохранено");
  };

  const changeLanguage = async (language: TranscriptLanguage) => {
    if (!settings) return;
    const next = { ...settings, language };
    if (await saveSettings(next)) toast.success("Сохранено");
  };

  const changeTheme = async (theme: Theme) => {
    if (!settings || theme === settings.theme) return;
    const next = { ...settings, theme };
    await saveSettings(next);
  };

  const prepareModel = async (engine: Engine) => {
    if (hasActiveJob) {
      setSettingsError(ACTIVE_JOB_SWITCH_HINT);
      return;
    }
    if (previewMode) {
      const nextStatuses: EngineStatuses = {
        ...modelStatuses,
        [engine]: { available: true, modelReady: true },
      };
      setModelStatuses(nextStatuses);
      setFailedEngine(null);
      const currentSettings = settingsRef.current;
      if (currentSettings) {
        await saveSettings({ ...currentSettings, engine });
      }
      return;
    }
    setBusyEngine(engine);
    setFailedEngine(null);
    setSettingsError(null);
    setModelProgress(1);
    setModelProgressDetail(null);
    setModelStage("downloading");
    try {
      await invoke("download_model_for_engine", { engine });
      setModelProgress(100);
      const nextStatuses = await refreshModelStatuses();
      if (!nextStatuses[engine]?.modelReady) {
        throw new Error(
          "Модель не прошла итоговую проверку после скачивания. Попробуйте ещё раз.",
        );
      }
      const currentSettings = settingsRef.current;
      if (!currentSettings) return;
      if (await saveSettings({ ...currentSettings, engine })) {
        toast.success("Модель скачана и выбрана");
      }
    } catch (e: unknown) {
      if (!isCancelledError(e)) {
        setFailedEngine(engine);
        setSettingsError(formatErrorDescription(e));
      }
    } finally {
      setBusyEngine(null);
    }
  };

  const deleteModel = async (engine: Engine) => {
    if (!modelStatuses[engine]?.modelReady) return;
    if (hasActiveJob) {
      setSettingsError(ACTIVE_JOB_DELETE_HINT);
      return;
    }
    const ok = window.confirm(
      `Вы уверены, что хотите удалить модель «${ENGINE_LABEL[engine]}»?\n\nТексты и настройки останутся на месте. Если модель понадобится снова, ее можно будет скачать заново.`,
    );
    if (!ok) return;

    if (previewMode) {
      setModelStatuses((prev) => ({
        ...prev,
        [engine]: { ...(prev[engine] ?? { available: true }), modelReady: false },
      }));
      setFailedEngine(null);
      return;
    }

    setDeletingEngine(engine);
    setSettingsError(null);
    try {
      await invoke("delete_model_for_engine", { engine });
      await refreshModelStatuses();
      setFailedEngine(null);
      setModelProgress(0);
      setModelProgressDetail(null);
      toast.success("Модель удалена");
    } catch (e: unknown) {
      setSettingsError(formatErrorDescription(e));
      await refreshModelStatuses();
    } finally {
      setDeletingEngine(null);
    }
  };

  const toggleSummarizer = async (enabled: boolean) => {
    if (!settings) return;
    const next = { ...settings, summarizer_enabled: enabled };
    await saveSettings(next);
  };

  const changeSummaryModel = async (summary_model: SummaryModel) => {
    if (!settings || summary_model === settings.summary_model) return;
    const next = { ...settings, summary_model };
    if (previewMode) {
      await saveSettings(next);
      setSummarizerStatus({ available: true, modelReady: false });
      setSummaryProgress(0);
      return;
    }
    setSummaryBusy(true);
    setSettingsError(null);
    try {
      if (!(await saveSettings(next))) return;
      setSummarizerStatus(await invoke<SummarizerStatus>("get_summarizer_status"));
      setSummaryProgress(0);
    } finally {
      setSummaryBusy(false);
    }
  };

  const toggleDictation = async (enabled: boolean) => {
    if (!settings) return;
    const next = { ...settings, dictation_enabled: enabled };
    await saveSettings(next);
  };

  const changeDictationShortcut = async (shortcut: string) => {
    if (!settings) return;
    const next = {
      ...settings,
      dictation_enabled: true,
      dictation_hold_key: shortcut,
    };
    if (await saveSettings(next)) {
      setShortcutHint(`Сочетание сохранено: ${displayShortcut(shortcut)}`);
    } else {
      setShortcutHint(null);
    }
    setCapturingShortcut(false);
  };

  const captureDictationShortcut = (
    e: ShortcutKeyboardEvent | ReactKeyboardEvent<HTMLButtonElement>,
  ) => {
    if (!capturingShortcut) return;
    e.preventDefault();
    e.stopPropagation();

    const key = normalizeShortcutKey(e.key, e.code);
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

  const verifyAccessibilityPermission = async () => {
    if (previewMode) {
      setAccessibilityPermission("granted");
      return;
    }
    setAccessibilityPermission("checking");
    try {
      const granted = await invoke<boolean>("check_accessibility_permission");
      setAccessibilityPermission(granted ? "granted" : "verify");
    } catch {
      setAccessibilityPermission("verify");
    }
  };

  const openAccessibilitySettings = async () => {
    if (previewMode) return;
    try {
      await openUrl(ACCESSIBILITY_SETTINGS_URL);
    } catch (e: unknown) {
      setSettingsError(
        `Не удалось открыть настройки macOS автоматически.\nОткройте вручную: Системные настройки → Конфиденциальность и безопасность → Универсальный доступ.\n${formatErrorDescription(e)}`,
      );
    }
  };

  const requestAccessibilityPermission = async () => {
    if (previewMode) {
      setAccessibilityPermission("granted");
      return;
    }
    setAccessibilityPermission("checking");
    try {
      const granted = await invoke<boolean>("request_accessibility_permission");
      setAccessibilityPermission(granted ? "granted" : "verify");
      if (!granted) {
        await openAccessibilitySettings();
      }
    } catch {
      setAccessibilityPermission("verify");
      await openAccessibilitySettings();
    }
  };

  useEffect(() => {
    if (!capturingShortcut) return;

    const handleWindowKeyDown = (e: globalThis.KeyboardEvent) => {
      captureDictationShortcut(e);
    };

    window.addEventListener("keydown", handleWindowKeyDown, { capture: true });
    return () => {
      window.removeEventListener("keydown", handleWindowKeyDown, { capture: true });
    };
  }, [capturingShortcut, settings]);

  const setupSummarizerEnv = async () => {
    if (previewMode) {
      setSummarizerStatus({ available: true, modelReady: false });
      setEnvSetupStatus(null);
      return;
    }
    setEnvSetupBusy(true);
    setSettingsError(null);
    setEnvSetupStatus("Подготовка…");
    try {
      await invoke("setup_summarizer_env");
      setEnvSetupStatus(null);
      await refreshSummarizerStatus();
    } catch (e: unknown) {
      setSettingsError(formatErrorDescription(e));
      setEnvSetupStatus(null);
    } finally {
      setEnvSetupBusy(false);
    }
  };

  const prepareSummarizerModel = async () => {
    if (previewMode) {
      setSummarizerStatus({ available: true, modelReady: true });
      setSummaryProgress(100);
      return;
    }
    setSummaryBusy(true);
    setSettingsError(null);
    setSummaryProgress(1);
    setSummaryStage("downloading");
    try {
      await invoke("download_summarizer_model");
      setSummaryProgress(100);
      await refreshSummarizerStatus();
      if (settings && !settings.summarizer_promo_seen) {
        const next = { ...settings, summarizer_promo_seen: true };
        await saveSettings(next);
      }
    } catch (e: unknown) {
      setSettingsError(formatErrorDescription(e));
    } finally {
      setSummaryBusy(false);
    }
  };

  const deleteSummarizerModel = async () => {
    if (!summarizerStatus?.modelReady) return;
    const summaryModel = settings?.summary_model ?? DEFAULT_SUMMARY_MODEL;
    const ok = window.confirm(
      `Удалить модель «${SUMMARY_MODEL_LABEL[summaryModel]}»?\n\nКонспекты не сохранятся, но транскрипции останутся на месте. При необходимости модель можно скачать заново.`,
    );
    if (!ok) return;
    if (previewMode) {
      setSummarizerStatus({ available: true, modelReady: false });
      setSummaryProgress(0);
      return;
    }
    setSummaryDeleting(true);
    setSettingsError(null);
    try {
      await invoke("delete_summarizer_model");
      await refreshSummarizerStatus();
      setSummaryProgress(0);
    } catch (e: unknown) {
      setSettingsError(formatErrorDescription(e));
    } finally {
      setSummaryDeleting(false);
    }
  };

  const openLogs = () => {
    if (!isTauriRuntime()) return;
    return invoke("open_logs");
  };

  if (!settings) return null;
  const summaryModel = settings.summary_model ?? DEFAULT_SUMMARY_MODEL;

  return (
    <Dialog
      open
      onOpenChange={(openState) => {
        if (!openState) onClose();
      }}
    >
      <DialogContent className="glass-modal settings-modal-shell flex max-h-[90vh] min-h-0 w-[min(780px,calc(100vw-2rem))] flex-col gap-0 overflow-hidden p-0 sm:max-w-none">
        <DialogHeader className="modal-band-top min-w-0 shrink-0 p-6 pr-12">
          <DialogTitle>Настройки</DialogTitle>
          <DialogDescription>
            Базовые параметры, модели и дополнительные функции.
          </DialogDescription>
        </DialogHeader>

        <div className="settings-layout min-h-0 min-w-0 flex-1">
          <SettingsTabs active={activeTab} onChange={setActiveTab} />

          <FieldGroup
            key={activeTab}
            className="settings-content settings-tab-motion min-h-0 min-w-0 flex-1 overflow-y-auto overflow-x-hidden"
          >
            {settingsError && (
              <Alert variant="destructive">
                <AlertDescription className="whitespace-pre-wrap break-words">
                  {settingsError}
                </AlertDescription>
              </Alert>
            )}

            {activeTab === "basic" && (
              <>
                <SaveFolderSection saveDir={settings.save_dir} onPickFolder={pickFolder} />
                <LanguageSettingsSection
                  language={settings.language}
                  onLanguageChange={changeLanguage}
                />
                <AppearanceSection
                  theme={settings.theme}
                  onThemeChange={changeTheme}
                />
              </>
            )}

            {activeTab === "models" && (
              <ModelSettingsSection
                settings={settings}
                modelStatuses={modelStatuses}
                busyEngine={busyEngine}
                failedEngine={failedEngine}
                deletingEngine={deletingEngine}
                modelProgress={modelProgress}
                modelProgressDetail={modelProgressDetail}
                modelStage={modelStage}
                hasActiveJob={hasActiveJob}
                onEngineChange={changeEngine}
                onPrepareModel={prepareModel}
                onDeleteModel={deleteModel}
              />
            )}

            {activeTab === "dictation" && (
              <DictationSettingsSection
                settings={settings}
                capturingShortcut={capturingShortcut}
                shortcutHint={shortcutHint}
                accessibilityPermission={accessibilityPermission}
                onStartCapture={() => {
                  setCapturingShortcut(true);
                  setShortcutHint("Нажмите новое сочетание клавиш…");
                }}
                onCaptureShortcut={captureDictationShortcut}
                onStopCapture={() => setCapturingShortcut(false)}
                onVerifyAccessibility={verifyAccessibilityPermission}
                onRequestAccessibility={requestAccessibilityPermission}
                onToggleDictation={toggleDictation}
              />
            )}

            {activeTab === "summary" && (
              <SummarySettingsSection
                enabled={settings.summarizer_enabled}
                summaryModel={summaryModel}
                status={summarizerStatus}
                busy={summaryBusy}
                deleting={summaryDeleting}
                progress={summaryProgress}
                stage={summaryStage}
                envSetupBusy={envSetupBusy}
                envSetupStatus={envSetupStatus}
                onToggle={toggleSummarizer}
                onSummaryModelChange={changeSummaryModel}
                onSetupEnv={setupSummarizerEnv}
                onPrepareModel={prepareSummarizerModel}
                onDeleteModel={deleteSummarizerModel}
              />
            )}

            {activeTab === "updates" && (
              <UpdatesSettingsSection updater={updater} onOpenLogs={openLogs} />
            )}
          </FieldGroup>
        </div>

        <SettingsFooter appVersion={appVersion} />
      </DialogContent>
    </Dialog>
  );
}
