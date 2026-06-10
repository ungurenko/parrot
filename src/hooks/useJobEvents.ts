import { useEffect } from "react";
import { cleanupTauriListeners, isTauriRuntime, listenInTauri } from "@/lib/runtime";
import type { Job, JobStage, SummaryStage } from "../types";

interface JobQueued { id: string; sourceName: string; }
interface JobProgress { id: string; stage: JobStage; percent: number; }
interface JobDone { id: string; text: string; outputPath: string; }
interface JobError { id: string; message: string; }
interface JobCanceled { id: string; }
interface JobTitle { id: string; sourceName: string; }
interface SummaryProgress { id: string; percent: number; stage: SummaryStage; }
interface SummaryDone { id: string; markdown: string; outputPath: string; }
interface SummaryError { id: string; message: string; }
interface SummaryCanceled { id: string; }

export type JobAction =
  | { type: "jobQueued"; payload: JobQueued }
  | { type: "jobProgress"; payload: JobProgress }
  | { type: "jobDone"; payload: JobDone }
  | { type: "jobError"; payload: JobError }
  | { type: "jobTitle"; payload: JobTitle }
  | { type: "jobCanceled"; payload: JobCanceled }
  | { type: "jobCanceling"; id: string }
  | { type: "summaryProgress"; payload: SummaryProgress }
  | { type: "summaryDone"; payload: SummaryDone }
  | { type: "summaryError"; payload: SummaryError }
  | { type: "summaryCanceled"; payload: SummaryCanceled }
  | { type: "historyLoaded"; job: Job };

export function jobsReducer(jobs: Job[], action: JobAction): Job[] {
  switch (action.type) {
    case "jobQueued": {
      const existing = jobs.find((j) => j.id === action.payload.id);
      if (existing) return jobs;
      return [
        ...jobs,
        {
          id: action.payload.id,
          sourceName: action.payload.sourceName,
          status: "queued",
          stage: null,
          percent: 0,
        },
      ];
    }
    case "jobProgress":
      return updateJob(jobs, action.payload.id, (job) =>
        job.status === "canceling" || job.status === "canceled"
          ? job
          : {
              ...job,
              status: "running",
              stage: action.payload.stage,
              percent: action.payload.percent,
            },
      );
    case "jobDone":
      return updateJob(jobs, action.payload.id, (job) => ({
        ...job,
        status: "done",
        percent: 100,
        stage: null,
        text: action.payload.text,
        outputPath: action.payload.outputPath,
      }));
    case "jobError":
      return updateJob(jobs, action.payload.id, (job) => ({
        ...job,
        status: "error",
        error: action.payload.message,
        stage: null,
      }));
    case "jobTitle":
      return updateJob(jobs, action.payload.id, (job) => ({
        ...job,
        sourceName: action.payload.sourceName,
      }));
    case "jobCanceled":
      return updateJob(jobs, action.payload.id, (job) => ({
        ...job,
        status: "canceled",
        error: undefined,
        stage: null,
        percent: 0,
      }));
    case "jobCanceling":
      return updateJob(jobs, action.id, (job) =>
        job.status === "queued" || job.status === "running"
          ? { ...job, status: "canceling", stage: null }
          : job,
      );
    case "summaryProgress":
      return updateJob(jobs, action.payload.id, (job) => ({
        ...job,
        summaryStatus: "generating",
        summaryPercent: action.payload.percent,
        summaryStage: action.payload.stage,
        summaryError: undefined,
      }));
    case "summaryDone":
      return updateJob(jobs, action.payload.id, (job) => ({
        ...job,
        summaryStatus: "done",
        summaryPercent: 100,
        summary: action.payload.markdown,
        summaryPath: action.payload.outputPath,
        summaryError: undefined,
      }));
    case "summaryError":
      return updateJob(jobs, action.payload.id, (job) => ({
        ...job,
        summaryStatus: "error",
        summaryError: action.payload.message,
        summaryStage: undefined,
      }));
    case "summaryCanceled":
      return updateJob(jobs, action.payload.id, (job) => ({
        ...job,
        summaryStatus: "idle",
        summaryStage: undefined,
        summaryPercent: 0,
        summaryError: undefined,
      }));
    case "historyLoaded": {
      const without = jobs.filter((j) => j.id !== action.job.id);
      return [action.job, ...without];
    }
  }
}

function updateJob(jobs: Job[], id: string, update: (job: Job) => Job): Job[] {
  let changed = false;
  const next = jobs.map((job) => {
    if (job.id !== id) return job;
    const updated = update(job);
    changed ||= updated !== job;
    return updated;
  });
  return changed ? next : jobs;
}

export function useJobEvents(
  dispatch: React.Dispatch<JobAction>,
  onDone?: (id: string) => void,
) {
  useEffect(() => {
    if (!isTauriRuntime()) return;
    const listeners = [
      listenInTauri<JobQueued>("job:queued", (e) => {
        dispatch({ type: "jobQueued", payload: e.payload });
      }),
      listenInTauri<JobProgress>("job:progress", (e) => {
        dispatch({ type: "jobProgress", payload: e.payload });
      }),
      listenInTauri<JobDone>("job:done", (e) => {
        dispatch({ type: "jobDone", payload: e.payload });
        onDone?.(e.payload.id);
      }),
      listenInTauri<JobError>("job:error", (e) => {
        dispatch({ type: "jobError", payload: e.payload });
      }),
      listenInTauri<JobTitle>("job:title", (e) => {
        dispatch({ type: "jobTitle", payload: e.payload });
      }),
      listenInTauri<JobCanceled>("job:canceled", (e) => {
        dispatch({ type: "jobCanceled", payload: e.payload });
      }),
      listenInTauri<SummaryProgress>("summary:progress", (e) => {
        dispatch({ type: "summaryProgress", payload: e.payload });
      }),
      listenInTauri<SummaryDone>("summary:done", (e) => {
        dispatch({ type: "summaryDone", payload: e.payload });
      }),
      listenInTauri<SummaryError>("summary:error", (e) => {
        dispatch({ type: "summaryError", payload: e.payload });
      }),
      listenInTauri<SummaryCanceled>("summary:canceled", (e) => {
        dispatch({ type: "summaryCanceled", payload: e.payload });
      }),
    ];

    return () => {
      cleanupTauriListeners(listeners);
    };
  }, [dispatch, onDone]);
}
