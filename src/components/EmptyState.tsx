import { DropZone } from "./DropZone";
import { YouTubeInput } from "./YouTubeInput";
import { HistoryList } from "./HistoryList";
import type { HistoryEntry } from "../types";
import { ChevronRightIcon, LockKeyholeIcon, LightbulbIcon } from "lucide-react";

interface Props {
  onFiles: (paths: string[]) => void;
  onYouTube: (url: string) => void;
  historyEntries?: HistoryEntry[];
  onOpenHistory?: (id: string) => void;
  onDeleteHistory?: (id: string) => void;
  onClearHistory?: () => void;
  onOpenSettings?: () => void;
}

export function EmptyState({
  onFiles,
  onYouTube,
  historyEntries = [],
  onOpenHistory,
  onDeleteHistory,
  onClearHistory,
  onOpenSettings,
}: Props) {
  return (
    <div className="empty-workspace min-h-0 flex-1">
      <div className="empty-main">
        <DropZone onFiles={onFiles} />
        <div className="youtube-divider">
          <span>…или вставьте ссылку на YouTube</span>
        </div>
        <YouTubeInput onSubmit={onYouTube} />
        <div className="hints">
          <span className="privacy-hint">
            <span className="privacy-badge" aria-hidden="true">
              <LockKeyholeIcon size={15} />
            </span>
            Всё обрабатывается локально — ничего не уходит в сеть.
          </span>
          <span className="kbd-row">
            <span className="muted">Быстрый старт:</span>
            <span className="kbd-key">⌘</span>
            <span className="kbd-key">O</span>
            <span className="muted">открыть файл</span>
          </span>
        </div>
      </div>
      <aside className="empty-side" aria-label="История и статус">
        {onOpenHistory && onDeleteHistory && onClearHistory && (
          <HistoryList
            entries={historyEntries}
            onOpen={onOpenHistory}
            onDelete={onDeleteHistory}
            onClear={onClearHistory}
          />
        )}
        <button
          type="button"
          className="side-status-card"
          onClick={onOpenSettings}
          aria-label="Открыть настройки движка"
        >
          <span className="side-status-dot" aria-hidden="true" />
          <div>
            <div className="side-status-title">Локальный движок активен</div>
            <div className="side-status-text">Готов к транскрипции</div>
          </div>
          <ChevronRightIcon size={18} aria-hidden="true" />
        </button>
        <div className="side-note">
          <LightbulbIcon size={17} aria-hidden="true" />
          <span>
            Поддерживаются аудио и видео файлы, а также ссылки на YouTube.
          </span>
        </div>
      </aside>
    </div>
  );
}
