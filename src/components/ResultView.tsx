import { invoke } from "@tauri-apps/api/core";
import type { Job } from "../types";

interface Props {
  job: Job | null;
}

export function ResultView({ job }: Props) {
  if (!job) {
    return (
      <div className="text-sm text-[var(--color-muted)] italic">
        Результат появится здесь после завершения задачи
      </div>
    );
  }

  if (job.status === "error") {
    return (
      <div className="text-sm text-red-400">
        <div className="font-medium mb-1">Ошибка</div>
        <div className="whitespace-pre-wrap">{job.error}</div>
      </div>
    );
  }

  if (job.status !== "done" || !job.text) {
    return (
      <div className="text-sm text-[var(--color-muted)] italic">
        Задача ещё не завершена…
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full min-h-0">
      <div className="flex items-center justify-between mb-2 shrink-0">
        <div className="text-xs text-[var(--color-muted)] truncate">
          {job.outputPath}
        </div>
        <div className="flex gap-2 shrink-0">
          <button
            className="text-xs px-3 py-1.5 rounded-md bg-[var(--color-panel)] border border-[var(--color-border)] hover:bg-[var(--color-panel-hover)]"
            onClick={() => navigator.clipboard.writeText(job.text ?? "")}
          >
            📋 Копировать
          </button>
          <button
            className="text-xs px-3 py-1.5 rounded-md bg-[var(--color-panel)] border border-[var(--color-border)] hover:bg-[var(--color-panel-hover)]"
            onClick={() => invoke("open_in_finder", { path: job.outputPath })}
          >
            📂 В Finder
          </button>
        </div>
      </div>
      <textarea
        readOnly
        value={job.text}
        className="flex-1 min-h-0 p-3 rounded-md bg-[var(--color-panel)] border border-[var(--color-border)] text-sm font-mono resize-none"
      />
    </div>
  );
}
