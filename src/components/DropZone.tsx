import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";

interface Props {
  onFiles: (paths: string[]) => void;
}

const AUDIO_EXTS = ["mp3", "wav", "m4a", "flac", "ogg", "opus", "aac", "wma"];
const VIDEO_EXTS = ["mp4", "mov", "mkv", "avi", "webm", "m4v"];

export function DropZone({ onFiles }: Props) {
  const [hovering, setHovering] = useState(false);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "over") {
          setHovering(true);
        } else if (event.payload.type === "drop") {
          setHovering(false);
          const paths = event.payload.paths.filter((p) => {
            const ext = p.split(".").pop()?.toLowerCase() ?? "";
            return AUDIO_EXTS.includes(ext) || VIDEO_EXTS.includes(ext);
          });
          if (paths.length > 0) onFiles(paths);
        } else {
          setHovering(false);
        }
      })
      .then((u) => (unlisten = u));
    return () => {
      unlisten?.();
    };
  }, [onFiles]);

  const pickFiles = useCallback(async () => {
    const result = await open({
      multiple: true,
      directory: false,
      filters: [
        {
          name: "Аудио и видео",
          extensions: [...AUDIO_EXTS, ...VIDEO_EXTS],
        },
      ],
    });
    if (!result) return;
    const paths = Array.isArray(result) ? result : [result];
    onFiles(paths as string[]);
  }, [onFiles]);

  return (
    <div
      className={`flex flex-col items-center justify-center rounded-xl border-2 border-dashed transition-all cursor-pointer h-44 ${
        hovering
          ? "border-[var(--color-accent)] bg-[var(--color-accent)]/10"
          : "border-[var(--color-border)] bg-[var(--color-panel)] hover:bg-[var(--color-panel-hover)]"
      }`}
      onClick={pickFiles}
    >
      <div className="text-4xl mb-2">🎙️</div>
      <div className="text-lg font-medium">Перетащите файл сюда</div>
      <div className="text-sm text-[var(--color-muted)] mt-1">
        или нажмите, чтобы выбрать — mp3, mp4, mov, m4a, flac, …
      </div>
    </div>
  );
}
