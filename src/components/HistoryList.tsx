import {
  FileAudioIcon,
  FileVideoIcon,
  HistoryIcon,
  RefreshCwIcon,
  Trash2Icon,
} from "lucide-react";
import { useMemo, useState } from "react";
import {
  matchesHistoryFilter,
  visibleHistoryFilters,
  type HistoryFilter,
} from "../lib/historyFilters";
import type { HistoryEntry } from "../types";

interface Props {
  entries: HistoryEntry[];
  onOpen: (id: string) => void;
  onDelete: (id: string) => void;
  onClear: () => void;
  onRepeat: (entry: HistoryEntry) => void;
}

const MONTHS_RU = [
  "января",
  "февраля",
  "марта",
  "апреля",
  "мая",
  "июня",
  "июля",
  "августа",
  "сентября",
  "октября",
  "ноября",
  "декабря",
];

function absoluteDate(iso: string): string {
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return "недавно";
  const d = new Date(then);
  const pad = (n: number) => String(n).padStart(2, "0");
  const time = `${pad(d.getHours())}:${pad(d.getMinutes())}`;
  const now = new Date();
  const sameDay = d.toDateString() === now.toDateString();
  const yesterday = new Date(now);
  yesterday.setDate(now.getDate() - 1);
  const isYesterday = d.toDateString() === yesterday.toDateString();
  if (sameDay) return `Сегодня, ${time}`;
  if (isYesterday) return `Вчера, ${time}`;
  if (d.getFullYear() === now.getFullYear()) {
    return `${d.getDate()} ${MONTHS_RU[d.getMonth()]}, ${time}`;
  }
  return `${d.getDate()} ${MONTHS_RU[d.getMonth()]} ${d.getFullYear()}`;
}

function dateGroup(iso: string): string {
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return "Недавно";
  const d = new Date(then);
  const now = new Date();
  const sameDay = d.toDateString() === now.toDateString();
  const yesterday = new Date(now);
  yesterday.setDate(now.getDate() - 1);
  const isYesterday = d.toDateString() === yesterday.toDateString();
  if (sameDay) return "Сегодня";
  if (isYesterday) return "Вчера";
  if (d.getFullYear() === now.getFullYear()) {
    return `${d.getDate()} ${MONTHS_RU[d.getMonth()]}`;
  }
  return `${d.getDate()} ${MONTHS_RU[d.getMonth()]} ${d.getFullYear()}`;
}

function mediaIcon(sourceName: string) {
  const ext = sourceName.split(".").pop()?.toLowerCase();
  if (ext && ["mp4", "mov", "mkv", "avi", "webm", "m4v"].includes(ext)) {
    return <FileVideoIcon size={20} aria-hidden="true" />;
  }
  return <FileAudioIcon size={20} aria-hidden="true" />;
}

export function HistoryList({
  entries,
  onOpen,
  onDelete,
  onClear,
  onRepeat,
}: Props) {
  const deleteWithConfirm = (id: string) => {
    const ok = window.confirm(
      "Убрать запись из истории Parrot?\n\nТекст на диске останется.",
    );
    if (ok) onDelete(id);
  };
  const [filter, setFilter] = useState<HistoryFilter>("all");
  const [query, setQuery] = useState("");
  const showSearch = entries.length >= 8;
  const availableFilters = useMemo(
    () => visibleHistoryFilters(entries),
    [entries],
  );
  const showFilters = availableFilters.length > 1;
  // The selected filter can go empty (its last entry was deleted) and drop out
  // of the offered pills — fall back to "all" instead of showing a blank list.
  const activeFilter = availableFilters.some((option) => option.id === filter)
    ? filter
    : "all";

  const filteredEntries = useMemo(() => {
    const normalizedQuery = showSearch ? query.trim().toLowerCase() : "";
    return entries.filter((entry) => {
      const matchesQuery =
        !normalizedQuery ||
        entry.sourceName.toLowerCase().includes(normalizedQuery);
      return matchesQuery && matchesHistoryFilter(entry, activeFilter);
    });
  }, [activeFilter, entries, query, showSearch]);

  const groupedEntries = useMemo(() => {
    const groups: Array<{ label: string; entries: HistoryEntry[] }> = [];
    for (const entry of filteredEntries) {
      const label = dateGroup(entry.createdAt);
      const last = groups[groups.length - 1];
      if (last?.label === label) {
        last.entries.push(entry);
      } else {
        groups.push({ label, entries: [entry] });
      }
    }
    return groups;
  }, [filteredEntries]);

  const clearWithConfirm = () => {
    const ok = window.confirm(
      "Очистить историю Parrot?\n\nТексты на диске останутся, но список в приложении станет пустым.",
    );
    if (ok) onClear();
  };

  return (
    <section className="history-list">
      <div className="history-head">
        <h2 className="history-heading">История</h2>
        {entries.length > 0 && (
          <button
            type="button"
            className="history-clear"
            onClick={clearWithConfirm}
            title="Удалить все записи из истории"
          >
            <HistoryIcon size={14} aria-hidden="true" />
            Очистить
          </button>
        )}
      </div>
      {entries.length > 0 && (showSearch || showFilters) && (
        <div className="history-tools">
          {showSearch && (
            <input
              className="history-search"
              type="search"
              placeholder="Найти запись"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              aria-label="Найти запись"
            />
          )}
          {showFilters && (
            <div
              className="history-filters"
              role="group"
              aria-label="Фильтр истории"
            >
              {availableFilters.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  aria-pressed={activeFilter === item.id}
                  onClick={() => setFilter(item.id)}
                >
                  {item.label}
                </button>
              ))}
            </div>
          )}
        </div>
      )}
      {entries.length === 0 && (
        <div className="history-empty">
          Последние транскрипции появятся здесь после первого файла.
        </div>
      )}
      {entries.length > 0 && filteredEntries.length === 0 && (
        <div className="history-empty">По этому запросу записей нет.</div>
      )}
      {groupedEntries.map((group) => (
        <div key={group.label} className="history-group">
          <div className="history-group-title">{group.label}</div>
          <ul className="history-items">
            {group.entries.map((entry) => (
              <li key={entry.id} className="history-item motion-history-item">
                <span className="history-media" aria-hidden="true">
                  {mediaIcon(entry.sourceName)}
                </span>
                <button
                  type="button"
                  className="history-item-main"
                  onClick={() => onOpen(entry.id)}
                  title={entry.sourceName}
                >
                  <span className="history-name">{entry.sourceName}</span>
                  <span className="history-meta">
                    <span>{absoluteDate(entry.createdAt)}</span>
                    {entry.summaryPath && <span>конспект</span>}
                    {entry.translationPath && <span>перевод</span>}
                  </span>
                </button>
                <button
                  type="button"
                  className="history-repeat"
                  onClick={() => onRepeat(entry)}
                  title={
                    entry.sourceKind && entry.sourceValue
                      ? "Повторить эту запись"
                      : "Повтор недоступен для старой записи"
                  }
                  aria-label="Повторить эту запись"
                  disabled={!entry.sourceKind || !entry.sourceValue}
                >
                  <RefreshCwIcon width={14} height={14} />
                </button>
                <button
                  type="button"
                  className="history-delete"
                  onClick={() => deleteWithConfirm(entry.id)}
                  title="Убрать из истории"
                  aria-label="Убрать из истории"
                >
                  <Trash2Icon width={14} height={14} />
                </button>
              </li>
            ))}
          </ul>
        </div>
      ))}
    </section>
  );
}
