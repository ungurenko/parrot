import { useState } from "react";

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
        <svg viewBox="0 0 24 24">
          <path d="M23 12s0-3.8-.5-5.6a3 3 0 0 0-2.1-2.1C18.6 3.8 12 3.8 12 3.8s-6.6 0-8.4.5A3 3 0 0 0 1.5 6.4C1 8.2 1 12 1 12s0 3.8.5 5.6a3 3 0 0 0 2.1 2.1c1.8.5 8.4.5 8.4.5s6.6 0 8.4-.5a3 3 0 0 0 2.1-2.1C23 15.8 23 12 23 12zM9.8 15.6V8.4L15.8 12l-6 3.6z" />
        </svg>
      </span>
      <input
        type="url"
        placeholder="…или вставьте ссылку на YouTube"
        spellCheck={false}
        value={url}
        onChange={(e) => setUrl(e.target.value)}
      />
      <button type="submit" className="btn-primary" disabled={!url.trim()}>
        Старт →
      </button>
    </form>
  );
}
