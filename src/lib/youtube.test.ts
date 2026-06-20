import { describe, expect, it } from "vitest";
import { youtubeValidation } from "./youtube";

describe("youtubeValidation", () => {
  it("accepts normal YouTube links", () => {
    expect(youtubeValidation("https://www.youtube.com/watch?v=abc123").ok).toBe(
      true,
    );
    expect(youtubeValidation("https://youtu.be/abc123").ok).toBe(true);
  });

  it("rejects empty and lookalike links with user-facing text", () => {
    expect(youtubeValidation("").message).toBe("Вставьте ссылку на YouTube.");
    expect(youtubeValidation("https://not-youtube.example/watch?v=abc").ok).toBe(
      false,
    );
    expect(youtubeValidation("https://not-youtube.example/watch?v=abc").message).toBe(
      "Нужна ссылка youtube.com или youtu.be.",
    );
  });
});
