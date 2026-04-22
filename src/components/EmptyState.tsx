import { DropZone } from "./DropZone";
import { YouTubeInput } from "./YouTubeInput";
import { HistoryList } from "./HistoryList";
import type { HistoryEntry } from "../types";

interface Props {
  onFiles: (paths: string[]) => void;
  onYouTube: (url: string) => void;
  historyEntries?: HistoryEntry[];
  onOpenHistory?: (id: string) => void;
  onDeleteHistory?: (id: string) => void;
}

export function EmptyState({
  onFiles,
  onYouTube,
  historyEntries = [],
  onOpenHistory,
  onDeleteHistory,
}: Props) {
  return (
    <div className="flex flex-col gap-5">
      <DropZone onFiles={onFiles} />
      <YouTubeInput onSubmit={onYouTube} />
      <div className="hints">
        <span>Всё обрабатывается локально — ничего не уходит в сеть.</span>
        <span className="kbd-row">
          <span className="kbd-key">⌘</span>
          <span className="kbd-key">O</span>
          <span className="muted mx-2">открыть файл</span>
          <span className="kbd-key">⌘</span>
          <span className="kbd-key">V</span>
          <span className="muted ml-2">вставить URL</span>
        </span>
      </div>
      {onOpenHistory && onDeleteHistory && (
        <HistoryList
          entries={historyEntries}
          onOpen={onOpenHistory}
          onDelete={onDeleteHistory}
        />
      )}
    </div>
  );
}
