import {
  FileAudioIcon,
  FileVideoIcon,
  HistoryIcon,
  MoreHorizontalIcon,
  RefreshCwIcon,
  Trash2Icon,
} from "lucide-react";
import { useMemo, useState } from "react";
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

type HistoryFilter = "all" | "summary" | "youtube" | "files";

const FILTERS: Array<{ id: HistoryFilter; label: string }> = [
  { id: "all", label: "Все" },
  { id: "summary", label: "С конспектом" },
  { id: "youtube", label: "YouTube" },
  { id: "files", label: "Файлы" },
];

export function HistoryList({
  entries,
  onOpen,
  onDelete,
  onClear,
  onRepeat,
}: Props) {
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<HistoryFilter>("all");

  const filteredEntries = useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase();
    return entries.filter((entry) => {
      const matchesQuery =
        !normalizedQuery ||
        entry.sourceName.toLowerCase().includes(normalizedQuery);
      const matchesFilter =
        filter === "all" ||
        (filter === "summary" && Boolean(entry.summaryPath)) ||
        (filter === "youtube" && entry.sourceKind === "youtube") ||
        (filter === "files" && entry.sourceKind !== "youtube");
      return matchesQuery && matchesFilter;
    });
  }, [entries, filter, query]);

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
      {entries.length > 0 && (
        <div className="history-tools">
          <input
            className="history-search"
            type="search"
            placeholder="Найти запись"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
          <div className="history-filters" role="tablist" aria-label="Фильтр истории">
            {FILTERS.map((item) => (
              <button
                key={item.id}
                type="button"
                className={filter === item.id ? "active" : ""}
                onClick={() => setFilter(item.id)}
              >
                {item.label}
              </button>
            ))}
          </div>
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
              <li key={entry.id} className="history-item">
                <span className="history-media" aria-hidden="true">
                  {mediaIcon(entry.sourceName)}
                </span>
                <button
                  type="button"
                  className="history-item-main"
                  onClick={() => onOpen(entry.id)}
                  title="Открыть эту транскрипцию"
                >
                  <span className="history-name">{entry.sourceName}</span>
                  <span className="history-meta">
                    <span>{absoluteDate(entry.createdAt)}</span>
                    {entry.summaryPath && <span>конспект</span>}
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
                <span className="history-more" aria-hidden="true">
                  <MoreHorizontalIcon width={16} height={16} />
                </span>
                <button
                  type="button"
                  className="history-delete"
                  onClick={() => onDelete(entry.id)}
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
