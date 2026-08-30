import { describe, expect, it } from "vitest";
import type { Job } from "../types";
import { jobsReducer } from "./useJobEvents";

describe("jobsReducer", () => {
  it("keeps source, mode and language from the queued event", () => {
    const jobs = jobsReducer([], {
      type: "jobQueued",
      payload: {
        id: "job-1",
        sourceName: "https://youtu.be/example",
        sourceKind: "youtube",
        engine: "qwen-1.7b",
        language: "ru",
      },
    });

    expect(jobs[0]).toMatchObject({
      sourceKind: "youtube",
      engine: "qwen-1.7b",
      language: "ru",
      status: "queued",
    });
  });

  it("preserves the visible stage while cancellation starts", () => {
    const job: Job = {
      id: "job-1",
      sourceName: "Встреча.m4a",
      sourceKind: "localFile",
      status: "running",
      stage: "transcribing",
      percent: 64,
      engine: "parakeet",
      language: "ru",
    };

    const jobs = jobsReducer([job], { type: "jobCanceling", id: job.id });

    expect(jobs[0]).toMatchObject({
      status: "canceling",
      stage: "transcribing",
      percent: 64,
    });
  });

  it("does not move known progress backwards within one stage", () => {
    const job: Job = {
      id: "job-1",
      sourceName: "Встреча.m4a",
      sourceKind: "localFile",
      status: "running",
      stage: "transcribing",
      percent: 100,
      engine: "qwen-0.6b",
      language: "ru",
    };

    const jobs = jobsReducer([job], {
      type: "jobProgress",
      payload: { id: job.id, stage: "transcribing", percent: 95 },
    });

    expect(jobs[0].percent).toBe(100);
  });

  it("keeps progress inside the visible zero-to-one-hundred range", () => {
    const job: Job = {
      id: "job-1",
      sourceName: "Встреча.m4a",
      sourceKind: "localFile",
      status: "queued",
      stage: null,
      percent: 0,
      engine: "parakeet",
      language: "ru",
    };

    const tooHigh = jobsReducer([job], {
      type: "jobProgress",
      payload: { id: job.id, stage: "transcribing", percent: 140 },
    });
    const belowZero = jobsReducer([{ ...job, id: "job-2" }], {
      type: "jobProgress",
      payload: { id: "job-2", stage: "transcribing", percent: -10 },
    });

    expect(tooHigh[0].percent).toBe(100);
    expect(belowZero[0].percent).toBe(0);
  });
});
