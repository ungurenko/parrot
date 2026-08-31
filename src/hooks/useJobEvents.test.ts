import { describe, expect, it } from "vitest";
import type { Job } from "../types";
import { jobsForQueue, jobsReducer } from "./useJobEvents";

describe("jobsReducer", () => {
  it("updates a loaded history entry through normal translation events", () => {
    const loaded: Job = {
      id: "history-1",
      sourceName: "Talk",
      status: "done",
      stage: null,
      percent: 100,
      text: "Hello",
      outputPath: "/tmp/talk.txt",
    };

    const jobs = jobsReducer([], { type: "historyLoaded", payload: loaded });
    const running = jobsReducer(jobs, {
      type: "translationProgress",
      payload: {
        id: loaded.id,
        percent: 10,
        stage: "translating",
        currentPart: 1,
        totalParts: 2,
      },
    });

    expect(running[0]).toMatchObject({
      id: loaded.id,
      translation: {
        status: "generating",
        percent: 10,
      },
    });
  });

  it("keeps only the selected history entry outside the live queue", () => {
    const live: Job = {
      id: "live-1",
      sourceName: "Current recording",
      status: "done",
      stage: null,
      percent: 100,
    };
    const firstHistory: Job = {
      id: "history-1",
      sourceName: "First archive",
      status: "done",
      stage: null,
      percent: 100,
    };
    const secondHistory: Job = {
      id: "history-2",
      sourceName: "Second archive",
      status: "done",
      stage: null,
      percent: 100,
    };

    const withFirst = jobsReducer([live], {
      type: "historyLoaded",
      payload: firstHistory,
    });
    const withSecond = jobsReducer(withFirst, {
      type: "historyLoaded",
      payload: secondHistory,
    });

    expect(withSecond.map((job) => job.id)).toEqual(["history-2", "live-1"]);
    expect(jobsForQueue(withSecond).map((job) => job.id)).toEqual(["live-1"]);
  });

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

  it("stores translation progress and the completed translation", () => {
    const job: Job = {
      id: "job-1",
      sourceName: "Talk",
      status: "done",
      stage: null,
      percent: 100,
      text: "Hello",
      outputPath: "/tmp/talk.txt",
    };

    const running = jobsReducer([job], {
      type: "translationProgress",
      payload: {
        id: job.id,
        percent: 45,
        stage: "translating",
        currentPart: 2,
        totalParts: 4,
      },
    });
    const done = jobsReducer(running, {
      type: "translationDone",
      payload: {
        id: job.id,
        text: "Привет",
        outputPath: "/tmp/talk.translation.ru.txt",
      },
    });

    expect(running[0]).toMatchObject({
      translation: {
        status: "generating",
        percent: 45,
        currentPart: 2,
        totalParts: 4,
      },
    });
    expect(done[0]).toMatchObject({
      translation: {
        status: "done",
        result: {
          content: "Привет",
          outputPath: "/tmp/talk.translation.ru.txt",
        },
      },
    });
  });

  it("restores the previous translation after cancellation", () => {
    const job: Job = {
      id: "job-1",
      sourceName: "Talk",
      status: "done",
      stage: null,
      percent: 100,
      translation: {
        status: "done",
        result: {
          content: "Старый перевод",
          outputPath: "/tmp/talk.translation.ru.txt",
        },
      },
    };

    const running = jobsReducer([job], {
      type: "translationProgress",
      payload: {
        id: job.id,
        percent: 20,
        stage: "translating",
        currentPart: 1,
        totalParts: 3,
      },
    });
    const canceled = jobsReducer(running, {
      type: "translationCanceled",
      payload: { id: job.id },
    });

    expect(canceled[0].translation).toEqual(job.translation);
  });
});
