export interface YouTubeValidation {
  ok: boolean;
  message: string;
}

const YOUTUBE_HOSTS = new Set([
  "youtube.com",
  "www.youtube.com",
  "m.youtube.com",
  "music.youtube.com",
  "youtu.be",
]);

export function youtubeValidation(value: string): YouTubeValidation {
  const trimmed = value.trim();
  if (!trimmed) {
    return { ok: false, message: "Вставьте ссылку на YouTube." };
  }

  try {
    const url = new URL(trimmed);
    const host = url.hostname.toLowerCase();
    if (!YOUTUBE_HOSTS.has(host)) {
      return { ok: false, message: "Нужна ссылка youtube.com или youtu.be." };
    }
    return { ok: true, message: "Ссылка подходит." };
  } catch {
    return { ok: false, message: "Ссылка выглядит неполной." };
  }
}
