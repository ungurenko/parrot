import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { Card, CardContent } from "@/components/ui/card";
import { cn } from "@/lib/utils";

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
    <Card
      role="button"
      tabIndex={0}
      onClick={pickFiles}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          pickFiles();
        }
      }}
      className={cn(
        "h-36 cursor-pointer border border-dashed bg-muted/25 shadow-none transition-colors",
        hovering
          ? "border-primary bg-background ring-2 ring-primary/10"
          : "border-border hover:bg-background",
      )}
    >
      <CardContent className="flex h-full flex-col items-center justify-center px-4 text-center">
        <div className="mb-2 text-3xl" aria-hidden="true">
          🎙️
        </div>
        <div className="text-base font-medium">Перетащите файл сюда</div>
        <div className="mt-1 text-sm text-muted-foreground">
          или нажмите, чтобы выбрать — mp3, mp4, mov, m4a, flac, …
        </div>
      </CardContent>
    </Card>
  );
}
