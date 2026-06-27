import { describe, expect, it } from "vitest";
import { formatUpdateError } from "./useAutoUpdate";

describe("formatUpdateError", () => {
  it("shows a useful update message before technical details", () => {
    const message = formatUpdateError(
      new Error("HTTP GET failed: https://example.com/latest.json timed out"),
    );

    expect(message.split("\n")[0]).toBe(
      "Parrot не смог скачать модель, обновление или видео.",
    );
    expect(message).toContain("Технические детали");
    expect(message).toContain("HTTP GET failed");
  });
});
