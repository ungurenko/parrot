import { Trash2Icon } from "lucide-react";
import { ENGINE_LABEL, type Engine, type HistoryEntry } from "../types";

interface Props {
  entries: HistoryEntry[];
  onOpen: (id: string) => void;
  onDelete: (id: string) => void;
}

function relativeTime(iso: string): string {
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return "недавно";
  const diffSec = Math.max(0, Math.floor((Date.now() - then) / 1000));
  if (diffSec < 60) return "только что";
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return `${diffMin} мин. назад`;
  const diffHr = Math.floor(diffMin / 60);
  if (diffHr < 24) return `${diffHr} ч. назад`;
  const diffDay = Math.floor(diffHr / 24);
  if (diffDay === 1) return "вчера";
  if (diffDay < 7) return `${diffDay} дн. назад`;
  // Fall back to DD.MM.YYYY
  const d = new Date(then);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(d.getDate())}.${pad(d.getMonth() + 1)}.${d.getFullYear()}`;
}

function engineLabel(engine: string): string {
  if (engine in ENGINE_LABEL) {
    return ENGINE_LABEL[engine as Engine];
  }
  return engine;
}

export function HistoryList({ entries, onOpen, onDelete }: Props) {
  if (entries.length === 0) return null;

  return (
    <section className="history-list">
      <h2 className="history-heading">История</h2>
      <ul className="history-items">
        {entries.map((entry) => (
          <li key={entry.id} className="history-item">
            <button
              type="button"
              className="history-item-main"
              onClick={() => onOpen(entry.id)}
              title="Открыть эту транскрипцию"
            >
              <span className="history-name">{entry.sourceName}</span>
              <span className="history-meta">
                <span>{relativeTime(entry.createdAt)}</span>
                <span className="history-sep">·</span>
                <span>{engineLabel(entry.engine)}</span>
                {entry.summaryPath && (
                  <>
                    <span className="history-sep">·</span>
                    <span className="history-badge">🪶 конспект</span>
                  </>
                )}
              </span>
            </button>
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
