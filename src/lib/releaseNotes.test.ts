import { describe, expect, it } from "vitest";
import { parseReleaseNotes } from "./releaseNotes";

const REAL_BODY = [
  "## 🎯 Что нового",
  "",
  "Транскрибация стала заметно быстрее. Модель распознавания теперь постоянно живёт в памяти.",
  "",
  "Parrot сам подстраивается под конкретный Mac.",
  "",
  "## 📦 Как получить обновление",
  "",
  "Откройте Parrot и установите обновление из появившегося уведомления.",
].join("\n");

describe("parseReleaseNotes", () => {
  it("drops the install section and takes the first sentence as summary", () => {
    const parsed = parseReleaseNotes(REAL_BODY);

    expect(parsed.summary).toBe("Транскрибация стала заметно быстрее.");
    expect(parsed.details).toEqual([
      "Транскрибация стала заметно быстрее. Модель распознавания теперь постоянно живёт в памяти.",
      "Parrot сам подстраивается под конкретный Mac.",
    ]);
  });

  it("uses the whole text when there are no ## sections", () => {
    const parsed = parseReleaseNotes("Первая строка.\n\nВторой абзац.");

    expect(parsed.summary).toBe("Первая строка.");
    expect(parsed.details).toEqual(["Первая строка.", "Второй абзац."]);
  });

  it("truncates an overlong first sentence with an ellipsis", () => {
    const long = `${"Слово ".repeat(60).trim()}.`;
    const parsed = parseReleaseNotes(long);

    expect(parsed.summary.endsWith("…")).toBe(true);
    expect(parsed.summary.length).toBeLessThanOrEqual(141);
  });

  it("returns an empty result for missing or blank body", () => {
    expect(parseReleaseNotes(undefined)).toEqual({ summary: "", details: [] });
    expect(parseReleaseNotes(null)).toEqual({ summary: "", details: [] });
  });
});
