import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { cn } from "@/lib/utils";
import {
  isSlowModelDownload,
  modelDownloadDetails,
} from "@/lib/modelProgress";
import { DownloadIcon, Trash2Icon } from "lucide-react";
import {
  ENGINE_SIZE,
  isQwenEngine,
  type Engine,
  type EngineStatuses,
  type ModelProgressDetail,
} from "../types";

interface Props {
  value: Engine;
  statuses?: EngineStatuses;
  busyEngine?: Engine | null;
  deletingEngine?: Engine | null;
  progress?: number;
  progressDetail?: ModelProgressDetail | null;
  stage?: "downloading" | "warmup" | "ready";
  onChange: (engine: Engine) => void;
  onPrepare?: (engine: Engine) => void;
  onDelete?: (engine: Engine) => void;
}

const OPTIONS: Array<{
  id: Engine;
  title: string;
  hint: string;
  badge?: string;
}> = [
  {
    id: "parakeet",
    title: "Parakeet V3",
    hint: "Самый быстрый вариант для диктовки. Поддерживает русский и еще 24 языка.",
    badge: "быстрее всего",
  },
  {
    id: "qwen-0.6b",
    title: "Qwen3-ASR 0.6B MLX",
    hint: "Хорошее качество, но запуск тяжелее. Теперь работает через теплый локальный сервер.",
  },
  {
    id: "qwen-1.7b",
    title: "Qwen3-ASR 1.7B MLX",
    hint: "Максимальное качество на шумном/акцентном аудио. Нужен Mac с 32+ ГБ RAM для длинных файлов.",
    badge: "качество",
  },
  {
    id: "whisper",
    title: "Whisper Large-v3 turbo",
    hint: "Медленнее, но 100+ языков (включая азиатские и арабский).",
  },
];

const progressText = (
  progress: number,
  stage: "downloading" | "warmup" | "ready",
) => {
  if (progress >= 100 || stage === "ready") return "Готово";
  if (stage === "warmup") return `Прогреваю модель… ${progress}%`;
  return `Скачиваю модель… ${progress}%`;
};

export function EnginePicker({
  value,
  statuses = {},
  busyEngine = null,
  deletingEngine = null,
  progress = 0,
  progressDetail = null,
  stage = "downloading",
  onChange,
  onPrepare,
  onDelete,
}: Props) {
  const actionBusy = Boolean(busyEngine || deletingEngine);
  const showModelStatus = Boolean(onPrepare || onDelete);

  return (
    <div className="flex w-full min-w-0 flex-col gap-2 overflow-x-hidden">
      {OPTIONS.map((opt) => {
        const active = value === opt.id;
        const status = statuses[opt.id];
        const available = status?.available ?? true;
        const ready = Boolean(status?.modelReady);
        const preparing = busyEngine === opt.id;
        const deleting = deletingEngine === opt.id;
        const prepareLabel = isQwenEngine(opt.id)
          ? `Подготовить модель ${opt.title}`
          : `Скачать модель ${opt.title}`;
        const detailText = preparing ? modelDownloadDetails(progressDetail) : null;
        const slowDownload = preparing && isSlowModelDownload(progressDetail);

        return (
          <div
            key={opt.id}
            className={cn(
              "engine-card flex min-w-0 flex-col gap-2",
              active && "selected",
              !available && "unavailable",
            )}
          >
            <div className="flex min-w-0 flex-col gap-3 sm:flex-row sm:items-start">
              <button
                type="button"
                  onClick={() => onChange(opt.id)}
                  disabled={!available}
                  title={active ? "Эта модель выбрана" : "Выбрать эту модель"}
                  className="flex min-w-0 flex-1 items-start gap-3 rounded-md text-left outline-none disabled:cursor-not-allowed disabled:opacity-60 focus-visible:ring-3 focus-visible:ring-ring/50"
                >
                <span
                  className={cn(
                    "mt-0.5 flex size-5 shrink-0 items-center justify-center rounded-full border text-xs",
                    active
                      ? "border-primary bg-primary text-primary-foreground"
                      : "border-white/70 bg-white/46 text-muted-foreground",
                  )}
                  aria-hidden="true"
                >
                  {active ? "✓" : ""}
                </span>
                <span className="min-w-0">
                  <span className="flex min-w-0 flex-wrap items-center gap-2">
                    <span className="font-medium leading-snug">{opt.title}</span>
                    <Badge variant="outline">{ENGINE_SIZE[opt.id]}</Badge>
                    {opt.badge && <Badge variant="secondary">{opt.badge}</Badge>}
                    {active && (
                      <Badge variant="default" title="Эта модель сейчас выбрана">
                        Выбрана
                      </Badge>
                    )}
                    {!available && <Badge variant="secondary">Недоступна</Badge>}
                  </span>
                  <span className="mt-1 block whitespace-normal break-words text-xs leading-relaxed text-muted-foreground">
                    {status?.unavailableReason ?? opt.hint}
                  </span>
                </span>
              </button>

              <div className="flex shrink-0 items-center gap-2 self-start sm:self-auto">
                {showModelStatus && (
                  <span
                    className={cn("status-chip", ready && "ready")}
                    title={
                      ready
                        ? "Модель уже скачана на компьютер"
                        : "Модель еще не скачана на компьютер"
                    }
                  >
                    <span className="dot" aria-hidden="true" />
                    {ready ? "Скачана" : "Не скачана"}
                  </span>
                )}

                {!ready && onPrepare && available && (
                  <Button
                    type="button"
                    variant="outline"
                    size={preparing ? "sm" : "icon-sm"}
                    disabled={actionBusy}
                    onClick={() => onPrepare(opt.id)}
                    title={prepareLabel}
                    aria-label={prepareLabel}
                  >
                    {preparing ? (
                      `${progress}%`
                    ) : (
                      <DownloadIcon data-icon="inline-start" />
                    )}
                  </Button>
                )}

                {ready && onDelete && (
                  <Button
                    type="button"
                    variant="destructive"
                    size="icon-sm"
                    disabled={actionBusy}
                    onClick={() => onDelete(opt.id)}
                    title={`Удалить модель ${opt.title}`}
                    aria-label={`Удалить модель ${opt.title}`}
                  >
                    <Trash2Icon data-icon="inline-start" />
                  </Button>
                )}
              </div>
            </div>

            {preparing && (
              <div className="flex min-w-0 flex-col gap-2 pl-8">
                <Progress
                  value={Math.max(progress, 2)}
                  className={stage === "warmup" ? "animate-pulse" : ""}
                />
                <div className="break-words text-xs leading-relaxed text-muted-foreground">
                  {progressText(progress, stage)}
                </div>
                {stage === "downloading" && detailText && (
                  <div className="break-words text-xs leading-relaxed text-muted-foreground">
                    {detailText}
                  </div>
                )}
                {stage === "downloading" && slowDownload && (
                  <div className="break-words text-xs leading-relaxed text-muted-foreground">
                    Сервер отдает файл медленно, загрузка продолжается.
                  </div>
                )}
              </div>
            )}

            {deleting && (
              <div className="pl-8 text-xs text-muted-foreground">
                Удаляю модель…
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
