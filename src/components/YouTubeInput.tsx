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
    <form onSubmit={submit} className="flex gap-2 items-center">
      <span className="text-lg">📺</span>
      <input
        type="url"
        placeholder="YouTube URL…"
        value={url}
        onChange={(e) => setUrl(e.target.value)}
        className="flex-1 px-3 py-2 rounded-md bg-[var(--color-panel)] border border-[var(--color-border)] text-[var(--color-text)] focus:outline-none focus:border-[var(--color-accent)]"
      />
      <button
        type="submit"
        className="px-4 py-2 rounded-md bg-[var(--color-accent)] text-white font-medium hover:bg-[var(--color-accent-hover)] transition-colors disabled:opacity-50"
        disabled={!url.trim()}
      >
        Старт
      </button>
    </form>
  );
}
