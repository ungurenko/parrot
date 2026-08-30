import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import type { Job } from "../types";
import { ProcessingView } from "./ProcessingView";

function makeJob(overrides: Partial<Job> = {}): Job {
  return {
    id: "job-1",
    sourceName: "Интервью о локальной транскрибации.mp4",
    sourceKind: "localFile",
    status: "running",
    stage: "transcribing",
    percent: 42,
    engine: "parakeet",
    language: "ru",
    ...overrides,
  };
}

function renderProcessing(overrides: Partial<Job> = {}) {
  return renderToStaticMarkup(
    <ProcessingView job={makeJob(overrides)} onCancel={vi.fn()} />,
  );
}

describe("ProcessingView", () => {
  it("shows exact queued metadata and explains automatic start", () => {
    const html = renderProcessing({
      status: "queued",
      stage: null,
      percent: 0,
      sourceKind: "youtube",
      engine: "qwen-0.6b",
      language: "auto",
    });

    expect(html).toContain("YouTube");
    expect(html).toContain("Лучше для русского");
    expect(html).toContain("Авто");
    expect(html).toContain("Жду предыдущую задачу");
    expect(html).toContain("Parrot начнёт её автоматически");
    expect(html).toContain('aria-current="step"');
  });

  it("shows YouTube download progress and honest network context", () => {
    const html = renderProcessing({
      sourceName: "https://youtu.be/example",
      sourceKind: "youtube",
      stage: "downloading",
      percent: 18,
    });

    expect(html).toContain("Скачиваю аудио");
    expect(html).toContain("18%");
    expect(html).toContain('aria-valuenow="18"');
    expect(html).toContain("Сеть нужна только для загрузки YouTube");
  });

  it("keeps local-file preparation indeterminate and promises no source changes", () => {
    const html = renderProcessing({
      sourceKind: "localFile",
      stage: "extracting",
      percent: 0,
    });

    expect(html).toContain("Подготавливаю аудио");
    expect(html).toContain("Исходный файл останется без изменений");
    expect(html).not.toContain("aria-valuenow");
  });

  it("marks audio ready while transcription is active", () => {
    const html = renderProcessing({ stage: "transcribing", percent: 42 });

    expect(html).toContain("Распознаю речь");
    expect(html).toContain("42%");
    expect(html).toContain("Распознавание идёт локально на этом Mac");
    expect(html).toMatch(/processing-stage done[\s\S]*Аудио[\s\S]*готово/);
    expect(html).toMatch(
      /processing-stage active[\s\S]*Распознавание[\s\S]*42% готово/,
    );
  });

  it("uses a calm finishing status after 95 percent", () => {
    const html = renderProcessing({ stage: "transcribing", percent: 95 });

    expect(html).toContain("Заканчиваю распознавание");
    expect(html).toContain("Дорабатываю последнюю часть");
  });

  it("preserves the active stage while cancellation is in progress", () => {
    const html = renderProcessing({
      status: "canceling",
      stage: "transcribing",
      percent: 64,
    });

    expect(html).toContain("Останавливаю задачу");
    expect(html).toContain("64%");
    expect(html).toContain('aria-valuenow="64"');
    expect(html).toMatch(/<button[^>]*disabled=""[^>]*>[\s\S]*Отменяю…/);
    expect(html).toMatch(/processing-stage done[\s\S]*Аудио/);
    expect(html).toMatch(/processing-stage active[\s\S]*Распознавание/);
  });

  it("keeps a long source name available through its title", () => {
    const sourceName = "Очень длинное название записи ".repeat(8).trim();
    const html = renderProcessing({ sourceName });

    expect(html).toContain(`title="${sourceName}"`);
    expect(html).toContain(sourceName);
  });
});
