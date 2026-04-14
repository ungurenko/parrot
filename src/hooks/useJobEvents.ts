import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import type { Job, JobStage } from "../types";

interface JobQueued { id: string; sourceName: string; }
interface JobProgress { id: string; stage: JobStage; percent: number; }
interface JobDone { id: string; text: string; outputPath: string; }
interface JobError { id: string; message: string; }

export function useJobEvents(
  setJobs: React.Dispatch<React.SetStateAction<Job[]>>,
  onDone?: (job: Job) => void,
) {
  useEffect(() => {
    const unlisteners: Array<() => void> = [];

    listen<JobQueued>("job:queued", (e) => {
      setJobs((prev) => {
        const existing = prev.find((j) => j.id === e.payload.id);
        if (existing) return prev;
        return [
          ...prev,
          {
            id: e.payload.id,
            sourceName: e.payload.sourceName,
            status: "queued",
            stage: null,
            percent: 0,
          },
        ];
      });
    }).then((u) => unlisteners.push(u));

    listen<JobProgress>("job:progress", (e) => {
      setJobs((prev) =>
        prev.map((j) =>
          j.id === e.payload.id
            ? { ...j, status: "running", stage: e.payload.stage, percent: e.payload.percent }
            : j,
        ),
      );
    }).then((u) => unlisteners.push(u));

    listen<JobDone>("job:done", (e) => {
      setJobs((prev) => {
        const updated = prev.map((j) =>
          j.id === e.payload.id
            ? {
                ...j,
                status: "done" as const,
                percent: 100,
                stage: null,
                text: e.payload.text,
                outputPath: e.payload.outputPath,
              }
            : j,
        );
        const job = updated.find((j) => j.id === e.payload.id);
        if (job && onDone) onDone(job);
        return updated;
      });
    }).then((u) => unlisteners.push(u));

    listen<JobError>("job:error", (e) => {
      setJobs((prev) =>
        prev.map((j) =>
          j.id === e.payload.id
            ? { ...j, status: "error", error: e.payload.message, stage: null }
            : j,
        ),
      );
    }).then((u) => unlisteners.push(u));

    return () => {
      unlisteners.forEach((u) => u());
    };
  }, [setJobs, onDone]);
}
