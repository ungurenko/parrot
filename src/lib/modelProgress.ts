import type { ModelProgressDetail } from "../types";

const SLOW_SPEED_BYTES_PER_SEC = 2 * 1024 * 1024;

export function formatModelBytes(bytes: number) {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 МБ";
  const mb = bytes / (1024 * 1024);
  if (mb < 1024) return `${Math.round(mb)} МБ`;
  return `${(mb / 1024).toFixed(1)} ГБ`;
}

export function formatModelSpeed(bytesPerSecond: number) {
  if (!Number.isFinite(bytesPerSecond) || bytesPerSecond <= 0) {
    return "считаю скорость";
  }
  return `${formatModelBytes(bytesPerSecond)}/с`;
}

export function modelDownloadDetails(detail: ModelProgressDetail | null) {
  if (!detail || detail.total_bytes <= 0) return null;
  return `Скачано ${formatModelBytes(detail.downloaded_bytes)} из ${formatModelBytes(
    detail.total_bytes,
  )} · ${formatModelSpeed(detail.speed_bytes_per_sec)}`;
}

export function isSlowModelDownload(detail: ModelProgressDetail | null) {
  return (
    Boolean(detail) &&
    detail!.speed_bytes_per_sec > 0 &&
    detail!.speed_bytes_per_sec < SLOW_SPEED_BYTES_PER_SEC
  );
}
