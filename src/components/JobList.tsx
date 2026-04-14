import { invoke } from "@tauri-apps/api/core";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { Progress } from "@/components/ui/progress";
import { cn } from "@/lib/utils";
import type { Job } from "../types";

interface Props {
  jobs: Job[];
  onSelect: (job: Job) => void;
  selectedId: string | null;
}

const CANCELLED_MARKER = "Отменено пользователем";

function stageLabel(job: Job): string {
  if (job.status === "queued") return "В очереди";
  if (job.status === "done") return "Готово";
  if (job.status === "error") {
    return job.error === CANCELLED_MARKER ? "Отменено" : "Ошибка";
  }
  switch (job.stage) {
    case "preparing":
      return "Подготовка ссылки…";
    case "extracting":
      return job.percent > 0
        ? `Извлечение аудио ${job.percent}%`
        : "Извлечение аудио…";
    case "downloading":
      return job.percent > 0
        ? `Скачивание ${job.percent}%`
        : "Скачивание…";
    case "transcribing":
      return job.percent > 1
        ? `Транскрибация ${job.percent}%`
        : "Транскрибация началась…";
    default:
      return "…";
  }
}

function progressValue(job: Job): number {
  if (job.status !== "running") return 0;
  return Math.max(job.percent, 3);
}

function icon(job: Job): string {
  if (job.status === "queued") return "⏸";
  if (job.status === "done") return "✅";
  if (job.status === "error") {
    return job.error === CANCELLED_MARKER ? "🚫" : "⚠️";
  }
  return "⏳";
}

function badgeVariant(job: Job): React.ComponentProps<typeof Badge>["variant"] {
  if (job.status === "done") return "default";
  if (job.status === "error") return "destructive";
  if (job.status === "running") return "secondary";
  return "outline";
}

async function cancelJob(id: string) {
  try {
    await invoke("cancel_job", { id });
  } catch (e) {
    console.error("cancel_job failed:", e);
  }
}

export function JobList({ jobs, onSelect, selectedId }: Props) {
  if (jobs.length === 0) {
    return (
      <Empty className="min-h-40 border bg-background/60">
        <EmptyHeader>
          <EmptyMedia className="text-2xl">🕊️</EmptyMedia>
          <EmptyTitle>Очередь пуста</EmptyTitle>
          <EmptyDescription>
            Добавьте аудио, видео или ссылку YouTube.
          </EmptyDescription>
        </EmptyHeader>
      </Empty>
    );
  }

  return (
    <ul className="flex flex-col gap-1">
      {jobs.map((job) => (
        <li
          key={job.id}
          role="button"
          tabIndex={0}
          className={cn(
            "flex cursor-pointer items-center gap-3 rounded-md border border-transparent px-2.5 py-2 transition-colors",
            selectedId === job.id
              ? "border-border bg-background"
              : "hover:bg-background/70",
          )}
          onClick={() => onSelect(job)}
          onKeyDown={(event) => {
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault();
              onSelect(job);
            }
          }}
          title={job.error ?? undefined}
        >
          <span className="shrink-0 text-lg" aria-hidden="true">
            {icon(job)}
          </span>
          <div className="min-w-0 flex-1">
            <div className="truncate text-sm font-medium">{job.sourceName}</div>
            <div className="mt-1 flex items-center gap-2">
              <Badge variant={badgeVariant(job)}>{stageLabel(job)}</Badge>
            </div>
            {job.status === "running" && (
              <Progress value={progressValue(job)} className="mt-2 h-1.5" />
            )}
          </div>
          {(job.status === "queued" || job.status === "running") && (
            <Button
              variant="ghost"
              size="icon-xs"
              onClick={(e) => {
                e.stopPropagation();
                cancelJob(job.id);
              }}
              title="Отменить"
            >
              <span aria-hidden="true">✕</span>
              <span className="sr-only">Отменить</span>
            </Button>
          )}
          {job.status === "done" && job.outputPath && (
            <Button
              variant="ghost"
              size="icon-xs"
              onClick={(e) => {
                e.stopPropagation();
                invoke("open_in_finder", { path: job.outputPath });
              }}
              title="Показать в Finder"
            >
              <span aria-hidden="true">📂</span>
              <span className="sr-only">Показать в Finder</span>
            </Button>
          )}
        </li>
      ))}
    </ul>
  );
}
