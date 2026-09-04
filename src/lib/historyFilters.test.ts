import { describe, expect, it } from "vitest";
import type { HistoryEntry } from "../types";
import { matchesHistoryFilter, visibleHistoryFilters } from "./historyFilters";

function entry(overrides: Partial<HistoryEntry> = {}): HistoryEntry {
  return {
    id: "1",
    sourceName: "lecture.mp3",
    sourceKind: "localFile",
    engine: "parakeet",
    language: "ru",
    createdAt: "2026-09-04T10:00:00.000Z",
    outputPath: "/tmp/lecture.txt",
    ...overrides,
  };
}

describe("matchesHistoryFilter", () => {
  it("keeps every entry under 'all'", () => {
    expect(matchesHistoryFilter(entry(), "all")).toBe(true);
  });

  it("matches summary and translation by saved artifact path", () => {
    const withSummary = entry({ summaryPath: "/tmp/lecture.summary.md" });
    expect(matchesHistoryFilter(withSummary, "summary")).toBe(true);
    expect(matchesHistoryFilter(withSummary, "translation")).toBe(false);
  });

  it("splits YouTube entries from local files", () => {
    const fromYoutube = entry({ sourceKind: "youtube" });
    expect(matchesHistoryFilter(fromYoutube, "youtube")).toBe(true);
    expect(matchesHistoryFilter(fromYoutube, "files")).toBe(false);
    expect(matchesHistoryFilter(entry(), "files")).toBe(true);
  });

  it("treats an entry without sourceKind as a local file", () => {
    const legacy = entry({ sourceKind: undefined });
    expect(matchesHistoryFilter(legacy, "files")).toBe(true);
    expect(matchesHistoryFilter(legacy, "youtube")).toBe(false);
  });
});

describe("visibleHistoryFilters", () => {
  it("offers only 'all' for empty history", () => {
    expect(visibleHistoryFilters([]).map((option) => option.id)).toEqual(["all"]);
  });

  it("hides filters that would return nothing", () => {
    const entries = [entry({ summaryPath: "/tmp/lecture.summary.md" }), entry({ id: "2" })];
    expect(visibleHistoryFilters(entries).map((option) => option.id)).toEqual([
      "all",
      "summary",
      "files",
    ]);
  });

  it("keeps the declared order when everything is present", () => {
    const entries = [
      entry({ summaryPath: "/tmp/a.summary.md" }),
      entry({ id: "2", translationPath: "/tmp/b.translation.md" }),
      entry({ id: "3", sourceKind: "youtube" }),
    ];
    expect(visibleHistoryFilters(entries).map((option) => option.id)).toEqual([
      "all",
      "summary",
      "translation",
      "youtube",
      "files",
    ]);
  });
});
