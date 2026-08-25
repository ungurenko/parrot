import { useState } from "react";
import { PlayIcon } from "lucide-react";
import { youtubeValidation } from "@/lib/youtube";

interface Props {
  onSubmit: (url: string) => Promise<boolean>;
}

export function YouTubeInput({ onSubmit }: Props) {
  const [url, setUrl] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const validation = youtubeValidation(url);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = url.trim();
    if (!validation.ok || submitting) return;
    setSubmitting(true);
    try {
      const ok = await onSubmit(trimmed);
      if (ok) setUrl("");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="yt-field">
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
          name="youtube-url"
          placeholder="https://www.youtube.com/watch?v=..."
          spellCheck={false}
          autoComplete="off"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          aria-label="Ссылка на YouTube"
          aria-describedby="youtube-validation"
        />
        <button
          type="submit"
          className="btn-primary"
          disabled={!validation.ok || submitting}
        >
          <PlayIcon size={16} aria-hidden="true" />
          Расшифровать
        </button>
      </form>
      <div
        id="youtube-validation"
        className={validation.ok ? "yt-validation ok" : "yt-validation"}
      >
        {validation.message}
      </div>
    </div>
  );
}
