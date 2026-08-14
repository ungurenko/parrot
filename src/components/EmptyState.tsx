import { DropZone } from "./DropZone";
import { YouTubeInput } from "./YouTubeInput";
import { HistoryList } from "./HistoryList";
import type { HistoryEntry } from "../types";
import { LockKeyholeIcon } from "lucide-react";
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
            Локально{engineLabel ? ` · ${engineLabel}` : ""}
          </span>
          <span className="kbd-row">
            <span className="muted">Быстрый старт:</span>
            <span className="kbd-key">⌘</span>
            <span className="kbd-key">O</span>
            <span className="muted">открыть файл</span>
          </span>
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
        </aside>
      )}
    </div>
  );
}
