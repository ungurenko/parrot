import { useEffect, useMemo, useRef, useState } from "react";
import parrotImg from "/parrot.png";
import { processingProgressMessage } from "@/lib/progressEstimate";
import type { Job } from "../types";

interface Props {
  job: Job;
  onCancel: (id: string) => void;
}

const CANCELLED_MARKER = "Отменено пользователем";

function statusText(job: Job): string {
  if (job.status === "queued") return "В очереди…";
  if (job.status === "canceling") return "Отменяю…";
  if (job.status === "canceled") return "Отменено";
  if (job.status === "error") {
    return job.error === CANCELLED_MARKER ? "Отменено" : "Ошибка";
  }
  switch (job.stage) {
    case "preparing":
      return "Подготовка ссылки…";
    case "extracting":
      return job.percent > 0
        ? `Извлечение аудио · ${job.percent}%`
        : "Извлечение аудио…";
    case "downloading":
      return job.percent > 0
        ? `Скачивание · ${job.percent}%`
        : "Скачивание…";
    case "transcribing":
      return job.percent > 1
        ? `Слушаю · ${job.percent}%`
        : "Слушаю…";
    default:
      return "Слушаю…";
  }
}

const BAR_COUNT = 40;
const BAR_HEIGHTS = Array.from(
  { length: BAR_COUNT },
  (_, i) => 0.3 + 0.6 * Math.abs(Math.sin(i * 1.618 + 0.8)),
);

type StageKey = "prep" | "transcribe" | "save";

function pipelineForJob(job: Job): {
  current: StageKey;
  labels: Record<StageKey, { title: string; sub: string }>;
} {
  const isYoutube =
    job.stage === "downloading" || job.stage === "preparing";
  const prepTitle = isYoutube ? "Скачивание" : "Извлечение";
  const prepSub = isYoutube ? "yt-dlp" : "ffmpeg";

  let current: StageKey = "prep";
  if (job.stage === "transcribing") current = "transcribe";
  if (job.status === "done") current = "save";

  return {
    current,
    labels: {
      prep: { title: prepTitle, sub: prepSub },
      transcribe: { title: "Транскрибация", sub: "движок" },
      save: { title: "Сохранение", sub: ".txt" },
    },
  };
}

const TIPS: string[] = [
  "Всё обрабатывается локально — ни байта не уходит в сеть.",
  "Движок можно поменять в настройках — Parakeet быстрее, Whisper точнее.",
  "Модель уже загружена в память — следующие файлы пойдут ещё быстрее.",
  "Можно закинуть сразу несколько файлов — они встанут в очередь.",
  "Ошибки распознавания? Попробуйте Qwen3-ASR 1.7B для сложного аудио.",
];

function useRotatingTip(intervalMs = 5500): string {
  const [idx, setIdx] = useState(() => Math.floor(Math.random() * TIPS.length));
  useEffect(() => {
    const t = window.setInterval(() => {
      setIdx((i) => (i + 1) % TIPS.length);
    }, intervalMs);
    return () => window.clearInterval(t);
  }, [intervalMs]);
  return TIPS[idx];
}

function StageItem({
  state,
  title,
  sub,
  percent,
}: {
  state: "pending" | "active" | "done";
  title: string;
  sub: string;
  percent?: number;
}) {
  const cls =
    state === "active" ? "stage-item active" : state === "done" ? "stage-item done" : "stage-item";
  return (
    <div className={cls}>
      <div className="stage-head">
        <span className="bullet" aria-hidden="true" />
        <span>{title}</span>
      </div>
      <div className="stage-sub">
        {state === "active" && percent !== undefined && percent > 0
          ? `${sub} · ${percent}%`
          : state === "done"
            ? "готово"
            : sub}
      </div>
    </div>
  );
}

export function ProcessingView({ job, onCancel }: Props) {
  const startedAtRef = useRef(Date.now());
  const calmProgress = processingProgressMessage({
    stage: job.stage,
    percent: job.percent,
    elapsedMs: Date.now() - startedAtRef.current,
  });
  const nearingEnd =
    job.status === "running" &&
    job.stage === "transcribing" &&
    job.percent >= 95;
  const status = job.status === "running"
    ? nearingEnd
      ? "Почти готово…"
      : calmProgress.title
    : statusText(job);
  const percent = job.status === "running" ? Math.max(job.percent, 3) : 0;

  const bars = useMemo(() => {
    const activeCount = Math.round((percent / 100) * BAR_COUNT);
    return BAR_HEIGHTS.map((h, i) => ({
      height: h,
      active: i < activeCount,
    }));
  }, [percent]);

  const indeterminate =
    job.status === "running" && job.percent <= 1 && job.stage === null;

  const pipeline = pipelineForJob(job);
  const tip = useRotatingTip();

  const stageState = (key: StageKey): "pending" | "active" | "done" => {
    const order: StageKey[] = ["prep", "transcribe", "save"];
    const a = order.indexOf(pipeline.current);
    const b = order.indexOf(key);
    if (b < a) return "done";
    if (b === a) return "active";
    return "pending";
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-5 justify-center">
      <div className="proc-card">
        <div
          className="parrot-pulse"
          style={{ backgroundImage: `url(${parrotImg})` }}
          aria-hidden="true"
        />
        <div className="min-w-0">
          <h3 className="proc-filename" title={job.sourceName}>
            {job.sourceName}
          </h3>
          <p className="proc-status">{status}</p>
          {job.status === "running" && (
            <p className="proc-detail">
              {nearingEnd
                ? "Дорабатываю последнюю часть. Это нормально, если немного задержится."
                : calmProgress.detail}
            </p>
          )}
          <div className="progress-track">
            <div
              className={
                indeterminate ? "progress-fill indeterminate" : "progress-fill"
              }
              style={indeterminate ? undefined : { width: `${percent}%` }}
            />
          </div>
          <div className="waveform" aria-hidden="true">
            {bars.map((b, i) => (
              <span
                key={i}
                className={b.active ? "bar-w active" : "bar-w"}
                style={{ transform: `scaleY(${b.height})` }}
              />
            ))}
          </div>
        </div>
      </div>

      <div className="stages">
        <StageItem
          state={stageState("prep")}
          title={pipeline.labels.prep.title}
          sub={pipeline.labels.prep.sub}
          percent={
            job.stage === "extracting" || job.stage === "downloading"
              ? job.percent
              : undefined
          }
        />
        <StageItem
          state={stageState("transcribe")}
          title={pipeline.labels.transcribe.title}
          sub={pipeline.labels.transcribe.sub}
          percent={job.stage === "transcribing" ? job.percent : undefined}
        />
        <StageItem
          state={stageState("save")}
          title={pipeline.labels.save.title}
          sub={pipeline.labels.save.sub}
        />
      </div>

      <div className="tip-card" aria-live="polite">
        <span className="tip-icon" aria-hidden="true">
          💡
        </span>
        <span className="tip-text" key={tip}>
          {tip}
        </span>
      </div>

      <button
        type="button"
        className="ghost-btn self-start"
        disabled={job.status === "canceling"}
        onClick={() => onCancel(job.id)}
      >
        <svg
          width="12"
          height="12"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth={2.5}
          strokeLinecap="round"
        >
          <path d="M18 6L6 18M6 6l12 12" />
        </svg>
        {job.status === "canceling" ? "Отменяю…" : "Отмена"}
      </button>
    </div>
  );
}
