import { useState } from "react";
import { PlayIcon } from "lucide-react";

interface Props {
  onSubmit: (url: string) => void;
}

export function YouTubeInput({ onSubmit }: Props) {
  const [url, setUrl] = useState("");

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = url.trim();
    if (!trimmed) return;
    onSubmit(trimmed);
    setUrl("");
  };

  return (
    <form onSubmit={submit} className="yt-bar">
      <span className="yt-icon" aria-hidden="true">
        <svg viewBox="0 0 28 20" focusable="false">
          <path
            d="M27.4 3.1a3.5 3.5 0 0 0-2.5-2.5C22.7 0 14 0 14 0S5.3 0 3.1.6A3.5 3.5 0 0 0 .6 3.1C0 5.3 0 10 0 10s0 4.7.6 6.9a3.5 3.5 0 0 0 2.5 2.5C5.3 20 14 20 14 20s8.7 0 10.9-.6a3.5 3.5 0 0 0 2.5-2.5c.6-2.2.6-6.9.6-6.9s0-4.7-.6-6.9Z"
            fill="#ff0033"
          />
          <path d="M11.2 14.3 18.5 10l-7.3-4.3v8.6Z" fill="#fff" />
        </svg>
      </span>
      <input
        type="url"
        placeholder="https://www.youtube.com/watch?v=..."
        spellCheck={false}
        value={url}
        onChange={(e) => setUrl(e.target.value)}
      />
      <button type="submit" className="btn-primary" disabled={!url.trim()}>
        <PlayIcon size={16} aria-hidden="true" />
        Старт
      </button>
    </form>
  );
}
