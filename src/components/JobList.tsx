import { invoke } from "@tauri-apps/api/core";
import { formatErrorDescription, userErrorFrom } from "@/lib/userErrors";
import { cn } from "@/lib/utils";
import type { Job } from "../types";

interface Props {
  jobs: Job[];
  onSelect: (job: Job) => void;
  onCancel: (id: string) => void;
  selectedId: string | null;
}

const CANCELLED_MARKER = "Отменено пользователем";

function statusText(job: Job): string {
  if (job.status === "queued") return "В очереди";
  if (job.status === "canceling") return "Отменяю…";
  if (job.status === "canceled") return "Отменено";
  if (job.status === "done") return "Готово";
  if (job.status === "error") {
    return job.error === CANCELLED_MARKER ? "Отменено" : "Ошибка";
  }
  switch (job.stage) {
    case "preparing":
      return "Подготовка…";
    case "extracting":
      return job.percent > 0 ? `Извлечение · ${job.percent}%` : "Извлечение…";
    case "downloading":
      return job.percent > 0 ? `Скачивание · ${job.percent}%` : "Скачивание…";
    case "transcribing":
      return job.percent > 1
        ? `Транскрибация · ${job.percent}%`
        : "Транскрибация…";
    default:
      return "Обработка…";
  }
}

function ledClass(job: Job): string {
  if (job.status === "running" || job.status === "canceling") return "coral";
  if (job.status === "done") return ""; // green default
  return "idle";
}

function progressPercent(job: Job): number {
  if (job.status !== "running") return 0;
  return Math.max(job.percent, 3);
}

function rowTitle(job: Job): string | undefined {
  if (!job.error || job.error === CANCELLED_MARKER) return undefined;
  const friendly = userErrorFrom(job.error);
  return `${friendly.title}\n${formatErrorDescription(job.error)}`;
}

async function cancelJob(id: string) {
  try {
    await invoke("cancel_job", { id });
  } catch (e) {
    console.error("cancel_job failed:", e);
  }
}

export function JobList({ jobs, onSelect, onCancel, selectedId }: Props) {
  if (jobs.length === 0) {
    return (
      <div className="px-2 py-4 text-center text-xs text-[color:var(--ink-3)]">
        Очередь пуста.
      </div>
    );
  }

  return (
    <ul className="flex flex-col gap-2">
      {jobs.map((job) => {
        const pct = progressPercent(job);
        const canCancel =
          job.status === "queued" ||
          job.status === "running" ||
          job.status === "canceling";
        return (
          <li
            key={job.id}
            role="button"
            tabIndex={0}
            className={cn("job-row", selectedId === job.id && "active")}
            onClick={() => onSelect(job)}
            onKeyDown={(event) => {
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                onSelect(job);
              }
            }}
            title={rowTitle(job)}
          >
            <span className={cn("led", ledClass(job))} aria-hidden="true" />
            <div className="min-w-0">
              <div className="row-title">{job.sourceName}</div>
              <div className="row-status">{statusText(job)}</div>
              {job.status === "running" && (
                <div className="progress-track mt-1.5">
                  <div
                    className="progress-fill"
                    style={{ width: `${pct}%` }}
                  />
                </div>
              )}
            </div>
            <div className="flex items-center">
              {canCancel ? (
                <button
                  type="button"
                  className="icon-btn icon-btn-sm"
                  disabled={job.status === "canceling"}
                  onClick={(e) => {
                    e.stopPropagation();
                    onCancel(job.id);
                    cancelJob(job.id);
                  }}
                  title="Отменить"
                >
                  <svg
                    width="12"
                    height="12"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth={2.5}
                    strokeLinecap="round"
                  >
                    <path d="M18 6L6 18M6 6l12 12" />
                  </svg>
                </button>
              ) : job.status === "done" && job.outputPath ? (
                <button
                  type="button"
                  className="icon-btn icon-btn-sm"
                  onClick={(e) => {
                    e.stopPropagation();
                    invoke("open_in_finder", { path: job.outputPath });
                  }}
                  title="Показать в Finder"
                >
                  <svg
                    width="12"
                    height="12"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth={2.2}
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  >
                    <path d="M7 17L17 7M17 7H9M17 7v8" />
                  </svg>
                </button>
              ) : null}
            </div>
          </li>
        );
      })}
    </ul>
  );
}
