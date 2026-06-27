import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { cn } from "@/lib/utils";
import {
  isSlowModelDownload,
  modelDownloadDetails,
} from "@/lib/modelProgress";
import { formatErrorDescription } from "@/lib/userErrors";
import { ENGINE_MODES } from "@/lib/engineModes";
import { modelProgressMessage } from "@/lib/progressEstimate";
import { DownloadIcon, Trash2Icon } from "lucide-react";
import {
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
  hasActiveJob?: boolean;
  onChange: (engine: Engine) => void;
  onPrepare?: (engine: Engine) => void;
  onDelete?: (engine: Engine) => void;
}

const ACTIVE_JOB_SWITCH_HINT =
  "Дождитесь окончания транскрибации, чтобы сменить модель";
const ACTIVE_JOB_DELETE_HINT =
  "Дождитесь окончания транскрибации, чтобы удалить модель";

const progressText = (
  progress: number,
  stage: "downloading" | "warmup" | "ready",
) => {
  return modelProgressMessage({ stage, percent: progress }).title;
};

const unavailableText = (reason?: string | null) =>
  reason ? formatErrorDescription(reason) : null;

export function EnginePicker({
  value,
  statuses = {},
  busyEngine = null,
  deletingEngine = null,
  progress = 0,
  progressDetail = null,
  stage = "downloading",
  hasActiveJob = false,
  onChange,
  onPrepare,
  onDelete,
}: Props) {
  const actionBusy = Boolean(busyEngine || deletingEngine);
  const showModelStatus = Boolean(onPrepare || onDelete);

  return (
    <div className="flex w-full min-w-0 flex-col gap-2 overflow-x-hidden">
      {ENGINE_MODES.map((opt) => {
        const active = value === opt.engine;
        const status = statuses[opt.engine];
        const available = status?.available ?? true;
        const ready = Boolean(status?.modelReady);
        const preparing = busyEngine === opt.engine;
        const deleting = deletingEngine === opt.engine;
        const prepareLabel = `Подготовить режим «${opt.title}»`;
        const unavailable = unavailableText(status?.unavailableReason);
        const detailText = preparing ? modelDownloadDetails(progressDetail) : null;
        const slowDownload = preparing && isSlowModelDownload(progressDetail);
        const calmProgress = modelProgressMessage({
          stage,
          percent: progress,
          detail: progressDetail,
        });

        return (
          <div
            key={opt.engine}
            className={cn(
              "engine-card flex min-w-0 flex-col gap-2",
              active && "selected",
              !available && "unavailable",
            )}
          >
            <div className="flex min-w-0 flex-col gap-3 sm:flex-row sm:items-start">
              <button
                type="button"
                  onClick={() => onChange(opt.engine)}
                  disabled={!available || (hasActiveJob && !active)}
                  title={
                    hasActiveJob && !active
                      ? ACTIVE_JOB_SWITCH_HINT
                      : active
                        ? "Эта модель выбрана"
                        : "Выбрать эту модель"
                  }
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
                    <Badge variant="outline">{opt.size}</Badge>
                    {opt.badge && <Badge variant="secondary">{opt.badge}</Badge>}
                    {active && (
                      <Badge variant="default" title="Эта модель сейчас выбрана">
                        Выбрана
                      </Badge>
                    )}
                    {!available && <Badge variant="secondary">Недоступна</Badge>}
                  </span>
                  <span className="mt-1 block text-xs font-medium text-muted-foreground">
                    {opt.subtitle}
                  </span>
                  <span className="mt-1 block whitespace-normal break-words text-xs leading-relaxed text-muted-foreground">
                    {unavailable ?? opt.detail}
                  </span>
                  <span className="mt-1 block whitespace-normal break-words text-[11px] leading-relaxed text-muted-foreground/80">
                    Модель: {opt.technicalName}
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
                    onClick={() => onPrepare(opt.engine)}
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
                    disabled={actionBusy || hasActiveJob}
                    onClick={() => onDelete(opt.engine)}
                    title={
                      hasActiveJob
                        ? ACTIVE_JOB_DELETE_HINT
                        : `Удалить модель ${opt.title}`
                    }
                    aria-label={
                      hasActiveJob
                        ? ACTIVE_JOB_DELETE_HINT
                        : `Удалить модель ${opt.title}`
                    }
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
                <div className="break-words text-xs leading-relaxed text-muted-foreground">
                  {calmProgress.detail}
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
