import { useRef, type CSSProperties } from "react";
import {
  CheckIcon,
  FileAudioIcon,
  HourglassIcon,
  InfoIcon,
  LockKeyholeIcon,
  VideoIcon,
  XIcon,
} from "lucide-react";
import parrotImg from "/parrot.png";
import "./ProcessingView.css";
import { modeOptionForEngine } from "@/lib/engineModes";
import { processingProgressMessage } from "@/lib/progressEstimate";
import { LANGUAGE_LABEL, type Job } from "../types";

interface Props {
  job: Job;
  onCancel: (id: string) => void;
}

type StageKey = "audio" | "transcribe" | "result";
type StageState = "pending" | "active" | "done";

interface PhaseClock {
  key: string;
  startedAt: number;
}

type ProcessingRingStyle = CSSProperties & {
  "--processing-progress": string;
};

const PROCESSING_STAGES: ReadonlyArray<{ key: StageKey; title: string }> = [
  { key: "audio", title: "Аудио" },
  { key: "transcribe", title: "Распознавание" },
  { key: "result", title: "Результат" },
];

function isYoutubeJob(job: Job): boolean {
  return (
    job.sourceKind === "youtube" ||
    job.stage === "preparing" ||
    job.stage === "downloading" ||
    /^https?:\/\//i.test(job.sourceName)
  );
}

function processingContext(job: Job): string {
  if (job.status === "queued") {
    return "Задача уже в очереди — Parrot начнёт её автоматически.";
  }
  if (job.status === "canceling") {
    return "Останавливаю обработку и очищаю временные файлы.";
  }
  if (job.stage === "preparing" || job.stage === "downloading") {
    return "Сеть нужна только для загрузки YouTube. Распознавание продолжится на этом Mac.";
  }
  if (job.stage === "extracting") {
    return "Подготавливаю звуковую дорожку. Исходный файл останется без изменений.";
  }
  if (job.stage === "transcribing") {
    return "Распознавание идёт локально на этом Mac.";
  }
  return "Parrot готовит задачу к обработке.";
}

function currentStage(job: Job): StageKey {
  return job.stage === "transcribing" ? "transcribe" : "audio";
}

function stageState(job: Job, key: StageKey): StageState {
  const current = currentStage(job);
  if (key === "audio" && current === "transcribe") return "done";
  if (key === current) return "active";
  return "pending";
}

function stageDetail(job: Job, key: StageKey, state: StageState): string {
  if (state === "done") return "готово";
  if (key === "result") return "сохранится автоматически";
  if (key === "transcribe") {
    if (state === "pending") return "следующий этап";
    return job.percent > 1 ? `${job.percent}% готово` : "слушаю запись";
  }
  if (job.status === "queued") return "ждёт очереди";
  if (job.stage === "preparing") return "готовлю ссылку";
  if (job.stage === "downloading") {
    return job.percent > 1
      ? `${job.percent}% загружено`
      : "загружаю из YouTube";
  }
  if (job.stage === "extracting") return "готовлю из файла";
  return isYoutubeJob(job) ? "из YouTube" : "из файла";
}

function StageItem({
  index,
  state,
  title,
  detail,
}: {
  index: number;
  state: StageState;
  title: string;
  detail: string;
}) {
  return (
    <li
      className={`processing-stage ${state}`}
      aria-current={state === "active" ? "step" : undefined}
    >
      <span className="processing-stage-marker" aria-hidden="true">
        {state === "done" ? <CheckIcon size={14} strokeWidth={2.8} /> : index}
      </span>
      <span className="processing-stage-copy">
        <strong>{title}</strong>
        <span>{detail}</span>
      </span>
    </li>
  );
}

function ContextIcon({ job }: { job: Job }) {
  if (job.status === "queued") return <HourglassIcon size={16} />;
  if (job.stage === "transcribing") return <LockKeyholeIcon size={16} />;
  return <InfoIcon size={16} />;
}

export function ProcessingView({ job, onCancel }: Props) {
  const phaseKey = `${job.id}:${job.status}:${job.stage ?? "none"}`;
  const phaseClock = useRef<PhaseClock>({
    key: phaseKey,
    startedAt: Date.now(),
  });
  if (phaseClock.current.key !== phaseKey) {
    phaseClock.current = { key: phaseKey, startedAt: Date.now() };
  }

  const progress = Math.min(100, Math.max(0, job.percent));
  const progressKnown = job.stage !== "preparing" && progress > 1;
  const indeterminate = job.status === "running" && !progressKnown;
  const calmProgress = processingProgressMessage({
    stage: job.stage,
    percent: progress,
    elapsedMs: Date.now() - phaseClock.current.startedAt,
  });
  const nearingEnd =
    job.status === "running" &&
    job.stage === "transcribing" &&
    progress >= 95;
  const status =
    job.status === "queued"
      ? "Жду предыдущую задачу"
      : job.status === "canceling"
        ? "Останавливаю задачу"
        : calmProgress.title;
  const statusDetail =
    job.status === "queued"
      ? "Начну автоматически, когда очередь дойдёт до этой записи."
      : job.status === "canceling"
        ? "Завершаю текущую операцию."
        : nearingEnd
          ? "Дорабатываю последнюю часть. Это может занять немного времени."
          : calmProgress.detail;
  const modeLabel = job.engine
    ? modeOptionForEngine(job.engine).title
    : "Текущий режим";
  const languageLabel = LANGUAGE_LABEL[job.language ?? "auto"];
  const youtube = isYoutubeJob(job);
  const ringStyle: ProcessingRingStyle = {
    "--processing-progress": `${progress * 3.6}deg`,
  };

  return (
    <div className="processing-workspace">
      <article className="processing-card" aria-labelledby="processing-source-name">
        <header className="processing-head">
          <div className="processing-source">
            <span className="processing-source-badge">
              {youtube ? (
                <VideoIcon size={15} aria-hidden="true" />
              ) : (
                <FileAudioIcon size={15} aria-hidden="true" />
              )}
              {youtube ? "YouTube" : "Файл"}
            </span>
            <h2
              id="processing-source-name"
              className="processing-source-name"
              title={job.sourceName}
            >
              {job.sourceName}
            </h2>
          </div>

          <div className="processing-controls">
            <div className="processing-meta" aria-label="Параметры задачи">
              <span>{modeLabel}</span>
              <span>{languageLabel}</span>
            </div>
            <button
              type="button"
              className="processing-cancel"
              disabled={job.status === "canceling"}
              onClick={() => onCancel(job.id)}
            >
              <XIcon size={14} strokeWidth={2.4} aria-hidden="true" />
              {job.status === "canceling" ? "Отменяю…" : "Отмена"}
            </button>
          </div>
        </header>

        <div className="processing-focus">
          <div
            className={`processing-orbit${indeterminate ? " indeterminate" : ""}${
              job.status === "canceling" ? " stopping" : ""
            }`}
            style={ringStyle}
            role="progressbar"
            aria-label="Прогресс текущего этапа"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={progressKnown ? progress : undefined}
            aria-valuetext={progressKnown ? `${progress}%` : "В процессе"}
          >
            <span
              className="processing-avatar"
              style={{ backgroundImage: `url(${parrotImg})` }}
              aria-hidden="true"
            />
          </div>

          <div className="processing-message">
            <span className="processing-eyebrow">Текущий этап</span>
            <div role="status" aria-live="polite" aria-atomic="true">
              <h3
                key={`${job.status}:${job.stage}:${nearingEnd}`}
                className="motion-text-swap"
              >
                {status}
              </h3>
            </div>
            <div className="processing-progress-copy">
              {progressKnown && (
                <strong key={progress} className="motion-number-pop">
                  {progress}%
                </strong>
              )}
              <span key={statusDetail} className="motion-text-swap">
                {statusDetail}
              </span>
            </div>
          </div>
        </div>

        <ol className="processing-stages" aria-label="Этапы обработки">
          {PROCESSING_STAGES.map((stage, index) => {
            const state = stageState(job, stage.key);
            return (
              <StageItem
                key={stage.key}
                index={index + 1}
                state={state}
                title={stage.title}
                detail={stageDetail(job, stage.key, state)}
              />
            );
          })}
        </ol>

        <div className="processing-context">
          <span className="processing-context-icon" aria-hidden="true">
            <ContextIcon job={job} />
          </span>
          <span>{processingContext(job)}</span>
        </div>
      </article>
    </div>
  );
}
