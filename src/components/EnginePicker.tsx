import { invoke } from "@tauri-apps/api/core";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { cn } from "@/lib/utils";
import {
  isSlowModelDownload,
  modelDownloadDetails,
} from "@/lib/modelProgress";
import { ENGINE_MODES } from "@/lib/engineModes";
import { modelProgressMessage } from "@/lib/progressEstimate";
import { Trash2Icon } from "lucide-react";
import {
  ACTIVE_JOB_DELETE_HINT,
  ACTIVE_JOB_SWITCH_HINT,
  type Engine,
  type EngineStatuses,
  type ModelProgressDetail,
  type ModelStage,
} from "../types";

interface Props {
  value: Engine;
  statuses?: EngineStatuses;
  busyEngine?: Engine | null;
  failedEngine?: Engine | null;
  deletingEngine?: Engine | null;
  progress?: number;
  progressDetail?: ModelProgressDetail | null;
  stage?: ModelStage;
  hasActiveJob?: boolean;
  onChange: (engine: Engine) => void;
  onPrepare?: (engine: Engine) => void;
  onDelete?: (engine: Engine) => void;
}

const unavailableText = (reason?: string | null) =>
  reason?.trim() || null;

export function EnginePicker({
  value,
  statuses = {},
  busyEngine = null,
  failedEngine = null,
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
    <div className="engine-list flex w-full min-w-0 flex-col gap-2 overflow-x-hidden">
      {ENGINE_MODES.map((opt) => {
        const active = value === opt.engine;
        const status = statuses[opt.engine];
        const available = status?.available ?? true;
        const ready = Boolean(status?.modelReady);
        const selected = active && (ready || !showModelStatus);
        const preparing = busyEngine === opt.engine;
        const deleting = deletingEngine === opt.engine;
        const failed = failedEngine === opt.engine;
        const unavailable = unavailableText(status?.unavailableReason);
        const detailText = preparing ? modelDownloadDetails(progressDetail) : null;
        const slowDownload = preparing && isSlowModelDownload(progressDetail);
        const calmProgress = modelProgressMessage({
          stage,
          percent: progress,
          detail: progressDetail,
        });
        let actionLabel = "Скачать и выбрать";
        if (!available) {
          actionLabel = "Недоступна";
        } else if (preparing) {
          actionLabel =
            stage === "installing" ? "Устанавливаю…" : calmProgress.title;
        } else if (selected) {
          actionLabel = "Выбрана";
        } else if (ready || !showModelStatus) {
          actionLabel = "Выбрать";
        } else if (failed) {
          actionLabel = "Повторить";
        }
        const actionDisabled =
          !available || selected || actionBusy || hasActiveJob;
        const actionHint = hasActiveJob
          ? ACTIVE_JOB_SWITCH_HINT
          : `${actionLabel}: ${opt.title}`;
        const runAction = () => {
          if (ready || !showModelStatus) {
            onChange(opt.engine);
          } else {
            onPrepare?.(opt.engine);
          }
        };

        return (
          <div
            key={opt.engine}
            className={cn(
              "engine-card flex min-w-0 flex-col gap-2",
              selected && "selected",
              !available && "unavailable",
            )}
          >
            <div className="flex min-w-0 flex-col gap-2.5 sm:flex-row sm:items-start">
              <div className="flex min-w-0 flex-1 items-start gap-3">
                <span
                  className={cn(
                    "mt-0.5 flex size-5 shrink-0 items-center justify-center rounded-full border text-xs",
                    selected
                      ? "border-primary bg-primary text-primary-foreground"
                      : "border-hairline-strong bg-surface-2 text-muted-foreground",
                  )}
                  aria-hidden="true"
                >
                  {selected ? "✓" : ""}
                </span>
                <span className="min-w-0">
                  <span className="flex min-w-0 flex-wrap items-center gap-2">
                    <span className="font-medium leading-snug">{opt.title}</span>
                    <Badge variant="outline">{opt.size}</Badge>
                    {opt.badge && <Badge variant="secondary">{opt.badge}</Badge>}
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
              </div>

              <div className="flex shrink-0 flex-wrap items-center gap-2 self-start sm:max-w-[210px] sm:justify-end">
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

                <Button
                  type="button"
                  variant={selected ? "default" : "outline"}
                  size="sm"
                  disabled={actionDisabled}
                  onClick={runAction}
                  title={actionHint}
                  aria-label={`${actionLabel}: ${opt.title}`}
                >
                  {actionLabel}
                </Button>

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
                <Button
                  variant="ghost"
                  size="sm"
                  className="self-start px-2 text-xs"
                  onClick={() =>
                    void invoke("cancel_model_prepare", { engine: opt.engine }).catch(
                      () => {},
                    )
                  }
                >
                  Отменить
                </Button>
                <div className="break-words text-xs leading-relaxed text-muted-foreground">
                  {modelProgressMessage({ stage, percent: progress }).title}
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
