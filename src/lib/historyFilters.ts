import type { HistoryEntry } from "../types";

export type HistoryFilter =
  | "all"
  | "summary"
  | "translation"
  | "youtube"
  | "files";

export interface HistoryFilterOption {
  id: HistoryFilter;
  label: string;
}

// Labels stay short on purpose: the history column is ~250px wide, and longer
// wording pushes the pills onto a third ragged row.
export const HISTORY_FILTERS: HistoryFilterOption[] = [
  { id: "all", label: "Все" },
  { id: "summary", label: "Конспект" },
  { id: "translation", label: "Перевод" },
  { id: "youtube", label: "YouTube" },
  { id: "files", label: "Файлы" },
];

export function matchesHistoryFilter(
  entry: HistoryEntry,
  filter: HistoryFilter,
): boolean {
  switch (filter) {
    case "all":
      return true;
    case "summary":
      return Boolean(entry.summaryPath);
    case "translation":
      return Boolean(entry.translationPath);
    case "youtube":
      return entry.sourceKind === "youtube";
    case "files":
      return entry.sourceKind !== "youtube";
  }
}

/**
 * Filters worth offering for the given history: "Все" always, the rest only
 * when at least one entry would show up under them. A pill that leads to an
 * empty list is noise.
 */
export function visibleHistoryFilters(
  entries: HistoryEntry[],
): HistoryFilterOption[] {
  return HISTORY_FILTERS.filter(
    (option) =>
      option.id === "all" ||
      entries.some((entry) => matchesHistoryFilter(entry, option.id)),
  );
}
