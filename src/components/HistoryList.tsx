import {
  FileAudioIcon,
  FileVideoIcon,
  HistoryIcon,
  MoreHorizontalIcon,
  Trash2Icon,
} from "lucide-react";
import type { HistoryEntry } from "../types";

interface Props {
  entries: HistoryEntry[];
  onOpen: (id: string) => void;
  onDelete: (id: string) => void;
  onClear: () => void;
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

function mediaIcon(sourceName: string) {
  const ext = sourceName.split(".").pop()?.toLowerCase();
  if (ext && ["mp4", "mov", "mkv", "avi", "webm", "m4v"].includes(ext)) {
    return <FileVideoIcon size={20} aria-hidden="true" />;
  }
  return <FileAudioIcon size={20} aria-hidden="true" />;
}

export function HistoryList({ entries, onOpen, onDelete, onClear }: Props) {
  return (
    <section className="history-list">
      <div className="history-head">
        <h2 className="history-heading">История</h2>
        {entries.length > 0 && (
          <button
            type="button"
            className="history-clear"
            onClick={onClear}
            title="Удалить все записи из истории"
          >
            <HistoryIcon size={14} aria-hidden="true" />
            Очистить
          </button>
        )}
      </div>
      {entries.length === 0 && (
        <div className="history-empty">
          Последние транскрипции появятся здесь после первого файла.
        </div>
      )}
      <ul className="history-items">
        {entries.map((entry) => (
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
              </span>
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
    </section>
  );
}
