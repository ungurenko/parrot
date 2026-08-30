import { describe, expect, it } from "vitest";
import { modelProgressMessage, processingProgressMessage } from "./progressEstimate";

describe("progressEstimate", () => {
  it("shows calm waiting text when exact time is unknown", () => {
    expect(processingProgressMessage({ stage: null, percent: 0 }).detail).toBe(
      "Обычно это занимает несколько минут.",
    );
  });

  it("estimates remaining time from elapsed progress", () => {
    expect(
      processingProgressMessage({
        stage: "transcribing",
        percent: 50,
        elapsedMs: 240_000,
      }).detail,
    ).toBe("Осталось примерно 4 мин.");
  });

  it("stops showing an unstable estimate near completion", () => {
    const message = processingProgressMessage({
      stage: "transcribing",
      percent: 96,
      elapsedMs: 600_000,
    });

    expect(message.title).toBe("Заканчиваю распознавание");
    expect(message.detail).toBe("Обычно это занимает несколько минут.");
  });

  it("explains warmup and slow downloads in plain Russian", () => {
    expect(modelProgressMessage({ stage: "warmup", percent: 96 }).detail).toBe(
      "Модель загружается в память. Обычно это 10-30 секунд.",
    );
    expect(
      modelProgressMessage({
        stage: "downloading",
        percent: 12,
        speedBytesPerSec: 80_000,
      }).detail,
    ).toBe("Сеть медленная, но загрузка продолжается.");
  });
});
