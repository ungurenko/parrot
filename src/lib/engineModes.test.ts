import { describe, expect, it } from "vitest";
import {
  ENGINE_MODES,
  readableEngineName,
} from "./engineModes";

describe("engineModes", () => {
  it("keeps technical names as secondary copy", () => {
    const fast = ENGINE_MODES.find((mode) => mode.id === "fast");

    expect(fast?.title).toBe("Быстро");
    expect(fast?.technicalName).toBe("Parakeet V3");
    expect(fast?.primary).toBe(true);
  });

  it("shows the user-friendly name for a stored engine id", () => {
    expect(readableEngineName("qwen-1.7b")).toContain("Сложная запись");
  });
});
