import { describe, expect, it } from "vitest";
import {
  ENGINE_MODES,
  engineForMode,
  modeForEngine,
  readableEngineName,
} from "./engineModes";

describe("engineModes", () => {
  it("maps user-friendly modes to existing engines", () => {
    expect(engineForMode("fast")).toBe("parakeet");
    expect(engineForMode("russian")).toBe("qwen-0.6b");
    expect(engineForMode("hardAudio")).toBe("qwen-1.7b");
    expect(engineForMode("manyLanguages")).toBe("whisper");
  });

  it("keeps technical names as secondary copy", () => {
    const fast = ENGINE_MODES.find((mode) => mode.id === "fast");

    expect(fast?.title).toBe("Быстро");
    expect(fast?.technicalName).toBe("Parakeet V3");
    expect(fast?.primary).toBe(true);
  });

  it("can recover the mode from a stored engine id", () => {
    expect(modeForEngine("parakeet")).toBe("fast");
    expect(modeForEngine("qwen-0.6b")).toBe("russian");
    expect(readableEngineName("qwen-1.7b")).toContain("Сложная запись");
  });
});
