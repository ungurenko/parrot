import { useEffect } from "react";
import { cleanupTauriListeners, isTauriRuntime, listenInTauri } from "@/lib/runtime";
import type { Job, JobStage, SummaryStage, TranslationStage } from "../types";
import { translationResult } from "../lib/jobArtifacts";

interface JobQueued {
  id: string;
  sourceName: string;
  sourceKind: NonNullable<Job["sourceKind"]>;
  engine: NonNullable<Job["engine"]>;
  language: NonNullable<Job["language"]>;
}
interface JobProgress {
  id: string;
  stage: Exclude<JobStage, null>;
  percent: number;
}
interface JobDone {
  id: string;
  text: string;
  outputPath: string;
  sourceKind: NonNullable<Job["sourceKind"]>;
  sourceValue: string;
  engine: NonNullable<Job["engine"]>;
  language: NonNullable<Job["language"]>;
}
interface JobError { id: string; message: string; }
interface JobCanceled { id: string; }
interface JobTitle { id: string; sourceName: string; }
interface SummaryProgress { id: string; percent: number; stage: SummaryStage; }
interface SummaryDone { id: string; markdown: string; outputPath: string; }
interface SummaryError { id: string; message: string; }
interface SummaryCanceled { id: string; }
interface TranslationProgress {
  id: string;
  percent: number;
  stage: TranslationStage;
  currentPart: number;
  totalParts: number;
}
interface TranslationDone { id: string; text: string; outputPath: string; }
interface TranslationError { id: string; message: string; }
interface TranslationCanceled { id: string; }

export type JobAction =
  | { type: "historyLoaded"; payload: Job }
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
  | { type: "translationProgress"; payload: TranslationProgress }
  | { type: "translationDone"; payload: TranslationDone }
  | { type: "translationError"; payload: TranslationError }
  | { type: "translationCanceled"; payload: TranslationCanceled };

export function jobsReducer(jobs: Job[], action: JobAction): Job[] {
  switch (action.type) {
    case "historyLoaded": {
      const existing = jobs.find((job) => job.id === action.payload.id);
      const loaded = {
        ...action.payload,
        origin: existing?.origin ?? "history",
      } satisfies Job;
      return [
        loaded,
        ...jobs.filter(
          (job) => job.id !== loaded.id && job.origin !== "history",
        ),
      ];
    }
    case "jobQueued": {
      const existing = jobs.find((j) => j.id === action.payload.id);
      if (existing) return jobs;
      return [
        ...jobs,
        {
          id: action.payload.id,
          sourceName: action.payload.sourceName,
          sourceKind: action.payload.sourceKind,
          engine: action.payload.engine,
          language: action.payload.language,
          origin: "session",
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
              percent:
                job.stage === action.payload.stage
                  ? Math.max(job.percent, clampedPercent(action.payload.percent))
                  : clampedPercent(action.payload.percent),
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
        sourceKind: action.payload.sourceKind,
        sourceValue: action.payload.sourceValue,
        engine: action.payload.engine,
        language: action.payload.language,
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
          ? { ...job, status: "canceling" }
          : job,
      );
    case "summaryProgress":
      return updateJob(jobs, action.payload.id, (job) => ({
        ...job,
        summary: {
          status: "generating",
          percent: clampedPercent(action.payload.percent),
          stage: action.payload.stage,
        },
      }));
    case "summaryDone":
      return updateJob(jobs, action.payload.id, (job) => ({
        ...job,
        summary: {
          status: "done",
          result: {
            content: action.payload.markdown,
            outputPath: action.payload.outputPath,
          },
        },
      }));
    case "summaryError":
      return updateJob(jobs, action.payload.id, (job) => ({
        ...job,
        summary: { status: "error", message: action.payload.message },
      }));
    case "summaryCanceled":
      return updateJob(jobs, action.payload.id, (job) => ({
        ...job,
        summary: { status: "idle" },
      }));
    case "translationProgress":
      return updateJob(jobs, action.payload.id, (job) => ({
        ...job,
        translation: {
          status: "generating",
          stage: action.payload.stage,
          percent: clampedPercent(action.payload.percent),
          currentPart: action.payload.currentPart,
          totalParts: action.payload.totalParts,
          previous: translationResult(job.translation),
        },
      }));
    case "translationDone":
      return updateJob(jobs, action.payload.id, (job) => ({
        ...job,
        translation: {
          status: "done",
          result: {
            content: action.payload.text,
            outputPath: action.payload.outputPath,
          },
        },
      }));
    case "translationError":
      return updateJob(jobs, action.payload.id, (job) => ({
        ...job,
        translation: {
          status: "error",
          message: action.payload.message,
          previous: translationResult(job.translation),
        },
      }));
    case "translationCanceled":
      return updateJob(jobs, action.payload.id, (job) => {
        const previous = translationResult(job.translation);
        return {
          ...job,
          translation: previous
            ? { status: "done", result: previous }
            : { status: "idle" },
        };
      });
  }
}

export function jobsForQueue(jobs: Job[]): Job[] {
  return jobs.filter((job) => job.origin !== "history");
}

function clampedPercent(percent: number): number {
  return Math.min(100, Math.max(0, percent));
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
  onError?: (id: string, message: string) => void,
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
        onError?.(e.payload.id, e.payload.message);
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
        onError?.(e.payload.id, e.payload.message);
      }),
      listenInTauri<SummaryCanceled>("summary:canceled", (e) => {
        dispatch({ type: "summaryCanceled", payload: e.payload });
      }),
      listenInTauri<TranslationProgress>("translation:progress", (e) => {
        dispatch({ type: "translationProgress", payload: e.payload });
      }),
      listenInTauri<TranslationDone>("translation:done", (e) => {
        dispatch({ type: "translationDone", payload: e.payload });
      }),
      listenInTauri<TranslationError>("translation:error", (e) => {
        dispatch({ type: "translationError", payload: e.payload });
        onError?.(e.payload.id, e.payload.message);
      }),
      listenInTauri<TranslationCanceled>("translation:canceled", (e) => {
        dispatch({ type: "translationCanceled", payload: e.payload });
      }),
    ];

    return () => {
      cleanupTauriListeners(listeners);
    };
  }, [dispatch, onDone, onError]);
}
