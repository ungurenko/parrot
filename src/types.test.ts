import { describe, expect, it } from "vitest";
import { DEFAULT_SUMMARY_MODEL, SUMMARY_MODEL_BADGE } from "./types";

describe("summary model defaults", () => {
  it("uses the faster Gemma model by default", () => {
    expect(DEFAULT_SUMMARY_MODEL).toBe("gemma4-e2b");
    expect(SUMMARY_MODEL_BADGE[DEFAULT_SUMMARY_MODEL]).toBe("быстрая");
  });
});
