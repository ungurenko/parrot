import { DropZone } from "./DropZone";
import { YouTubeInput } from "./YouTubeInput";
import { HistoryList } from "./HistoryList";
import type { HistoryEntry } from "../types";
import {
  ChevronRightIcon,
  FolderOpenIcon,
  LockKeyholeIcon,
  Settings2Icon,
} from "lucide-react";
import { cn } from "@/lib/utils";

interface Props {
  onFiles: (paths: string[]) => void;
  onYouTube: (url: string) => void;
  engineLabel?: string;
  historyEntries?: HistoryEntry[];
  onOpenHistory?: (id: string) => void;
  onDeleteHistory?: (id: string) => void;
  onClearHistory?: () => void;
  onRepeatHistory?: (entry: HistoryEntry) => void;
  onOpenSettings?: () => void;
}

export function EmptyState({
  onFiles,
  onYouTube,
  engineLabel,
  historyEntries = [],
  onOpenHistory,
  onDeleteHistory,
  onClearHistory,
  onRepeatHistory,
  onOpenSettings,
}: Props) {
  const showHistory =
    historyEntries.length > 0 &&
    Boolean(onOpenHistory && onDeleteHistory && onClearHistory && onRepeatHistory);

  return (
    <div
      className={cn(
        "empty-workspace min-h-0 flex-1",
        showHistory ? "with-history" : "single-panel",
      )}
    >
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
        <div className="workspace-status-row">
          <span className="workspace-status-dot" aria-hidden="true" />
          <span className="workspace-status-copy">
            <strong>{engineLabel ?? "Локальный движок"}</strong>
            <span>готов к транскрипции</span>
          </span>
          <button
            type="button"
            className="workspace-status-action"
            onClick={onOpenSettings}
          >
            <Settings2Icon size={15} aria-hidden="true" />
            Настроить
          </button>
        </div>
        <div className="workspace-footnote">
          <FolderOpenIcon size={15} aria-hidden="true" />
          <span>Поддерживаются аудио, видео и ссылки на YouTube.</span>
        </div>
      </div>
      {showHistory && (
        <aside className="empty-side" aria-label="История">
          <HistoryList
            entries={historyEntries}
            onOpen={onOpenHistory!}
            onDelete={onDeleteHistory!}
            onClear={onClearHistory!}
            onRepeat={onRepeatHistory!}
          />
          <button
            type="button"
            className="side-status-card"
            onClick={onOpenSettings}
            aria-label="Открыть настройки движка"
          >
            <span className="side-status-dot" aria-hidden="true" />
            <div>
              <div className="side-status-title">{engineLabel ?? "Движок"}</div>
              <div className="side-status-text">Настройки распознавания</div>
            </div>
            <ChevronRightIcon size={18} aria-hidden="true" />
          </button>
        </aside>
      )}
    </div>
  );
}
