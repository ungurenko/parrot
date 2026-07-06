import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { FileAudioIcon, FileVideoIcon, FolderOpenIcon } from "lucide-react";
import parrotImg from "/parrot.png";
import { cn } from "@/lib/utils";
import { isTauriRuntime } from "@/lib/runtime";

interface Props {
  onFiles: (paths: string[]) => void;
}

const AUDIO_EXTS = ["mp3", "wav", "m4a", "flac", "ogg", "opus", "aac", "wma"];
const VIDEO_EXTS = ["mp4", "mov", "mkv", "avi", "webm", "m4v"];
const CHIP_FORMATS = [
  { label: "MP3", kind: "audio" },
  { label: "M4A", kind: "audio" },
  { label: "MP4", kind: "video" },
  { label: "MOV", kind: "video" },
  { label: "WAV", kind: "audio" },
] as const;

export function DropZone({ onFiles }: Props) {
  const [hovering, setHovering] = useState(false);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let unlisten: (() => void) | null = null;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "over") {
          setHovering(true);
        } else if (event.payload.type === "drop") {
          setHovering(false);
          const dropped = event.payload.paths;
          const paths = dropped.filter((p) => {
            const ext = p.split(".").pop()?.toLowerCase() ?? "";
            return AUDIO_EXTS.includes(ext) || VIDEO_EXTS.includes(ext);
          });
          if (paths.length > 0) {
            onFiles(paths);
          } else if (dropped.length > 0) {
            toast.error("Файл не подходит", {
              description:
                "Parrot понимает аудио и видео: MP3, M4A, WAV, MP4, MOV. Выберите другой файл.",
            });
          }
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
    if (!isTauriRuntime()) return;
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
      role="button"
      tabIndex={0}
      onClick={pickFiles}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          pickFiles();
        }
      }}
      className={cn("hero-drop", hovering && "drag")}
    >
      <div
        className="parrot-hero"
        style={{ backgroundImage: `url(${parrotImg})` }}
        aria-hidden="true"
      />

      <div className="hero-text">
        <h1>Перетащите файл для транскрипции</h1>
        <p>
          Локальная расшифровка аудио и видео на вашем&nbsp;Mac.
          <br />
          Без облаков, подписок и лишней возни.
        </p>
        <div className="format-chips">
          {CHIP_FORMATS.map((f) => (
            <span key={f.label} className="chip">
              {f.kind === "audio" ? (
                <FileAudioIcon size={13} aria-hidden="true" />
              ) : (
                <FileVideoIcon size={13} aria-hidden="true" />
              )}
              {f.label}
            </span>
          ))}
        </div>
      </div>

      <button
        type="button"
        className="choose-btn"
        onClick={(e) => {
          e.stopPropagation();
          pickFiles();
        }}
      >
        <FolderOpenIcon size={17} aria-hidden="true" />
        Выбрать файл
      </button>
    </div>
  );
}
