import { invoke } from "@tauri-apps/api/core";
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

function progressWidth(job: Job): string {
  if (job.status !== "running") return "0%";
  return `${Math.max(job.percent, 3)}%`;
}

function icon(job: Job): string {
  if (job.status === "queued") return "⏸";
  if (job.status === "done") return "✅";
  if (job.status === "error") {
    return job.error === CANCELLED_MARKER ? "🚫" : "⚠️";
  }
  return "⏳";
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
      <div className="text-sm text-[var(--color-muted)] italic px-1">
        Очередь пуста
      </div>
    );
  }

  return (
    <ul className="space-y-1">
      {jobs.map((job) => (
        <li
          key={job.id}
          className={`flex items-center gap-3 px-3 py-2 rounded-md cursor-pointer transition-colors ${
            selectedId === job.id
              ? "bg-[var(--color-accent)]/15"
              : "hover:bg-[var(--color-panel-hover)]"
          }`}
          onClick={() => onSelect(job)}
          title={job.error ?? undefined}
        >
          <span className="text-lg shrink-0">{icon(job)}</span>
          <div className="flex-1 min-w-0">
            <div className="truncate text-sm">{job.sourceName}</div>
            <div className="text-xs text-[var(--color-muted)]">
              {stageLabel(job)}
            </div>
            {job.status === "running" && (
              <div className="h-1 mt-1 bg-[var(--color-border)] rounded overflow-hidden">
                <div
                  className="h-full bg-[var(--color-accent)] transition-all"
                  style={{ width: progressWidth(job) }}
                />
              </div>
            )}
          </div>
          {(job.status === "queued" || job.status === "running") && (
            <button
              className="text-xs px-2 py-1 rounded hover:bg-red-500/20 text-[var(--color-muted)] hover:text-red-400"
              onClick={(e) => {
                e.stopPropagation();
                cancelJob(job.id);
              }}
              title="Отменить"
            >
              ✕
            </button>
          )}
          {job.status === "done" && job.outputPath && (
            <button
              className="text-xs px-2 py-1 rounded hover:bg-[var(--color-border)]"
              onClick={(e) => {
                e.stopPropagation();
                invoke("open_in_finder", { path: job.outputPath });
              }}
              title="Показать в Finder"
            >
              📂
            </button>
          )}
        </li>
      ))}
    </ul>
  );
}
