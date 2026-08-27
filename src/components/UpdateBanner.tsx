import { useState } from "react";
import {
  ArrowUpIcon,
  BellIcon,
  ChevronDownIcon,
  ChevronUpIcon,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import type { AutoUpdate } from "../hooks/useAutoUpdate";
import { parseReleaseNotes } from "@/lib/releaseNotes";
import { cn } from "@/lib/utils";

interface Props {
  updater: AutoUpdate;
  onDismiss: () => void;
  onOpenSettings: () => void;
}

interface UpdateBannerViewProps extends Props {
  expanded: boolean;
  onToggleExpanded: () => void;
}

export function UpdateBannerView({
  updater,
  expanded,
  onToggleExpanded,
  onDismiss,
  onOpenSettings,
}: UpdateBannerViewProps) {
  const { available, status, progress, errorScope } = updater;
  if (!available) return null;

  const installing = status === "installing";
  const installError = status === "error" && errorScope === "install";
  const notes = parseReleaseNotes(available.body);
  const hasNotes = notes.details.length > 0;

  return (
    <div
      className={cn("update-banner", expanded && "expanded")}
      role="region"
      aria-label="Обновление Parrot"
    >
      <div className="update-banner-icon" aria-hidden="true">
        <BellIcon size={15} />
      </div>
      <div className="update-banner-body">
        <div className="update-banner-title-row">
          <span className="update-banner-title">Доступно обновление Parrot</span>
          <Badge variant="secondary">v{available.version}</Badge>
          {!installing && !installError && hasNotes && (
            <button
              type="button"
              className="update-banner-notes-toggle"
              onClick={onToggleExpanded}
              aria-expanded={expanded}
              aria-controls="update-banner-notes"
            >
              Что нового
              {expanded ? (
                <ChevronUpIcon size={12} />
              ) : (
                <ChevronDownIcon size={12} />
              )}
            </button>
          )}
        </div>
        {installing ? (
          <Progress
            value={Math.max(progress, 2)}
            className="update-banner-progress"
          />
        ) : installError ? (
          <p className="update-banner-summary">
            Не удалось установить обновление. {" "}
            <button
              type="button"
              className="update-banner-subtitle underline underline-offset-2"
              onClick={onOpenSettings}
            >
              Подробнее — в Настройках
            </button>
          </p>
        ) : expanded && hasNotes ? (
          <>
            <div className="update-banner-meta">
              Меньше минуты · Перезапустится автоматически
            </div>
            <div id="update-banner-notes" className="update-banner-notes">
              {notes.details.map((paragraph, idx) => (
                <p key={idx}>{paragraph}</p>
              ))}
            </div>
          </>
        ) : null}
      </div>
      <div className="update-banner-actions">
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={updater.install}
          disabled={installing}
        >
          <ArrowUpIcon size={14} />
          {installing
            ? progress > 0
              ? `Обновляю… ${progress}%`
              : "Готовлю…"
            : installError
              ? "Повторить"
              : "Обновить"}
        </Button>
        <button
          type="button"
          className="update-banner-close"
          onClick={onDismiss}
          aria-label="Скрыть подсказку"
          title="Скрыть до следующего запуска"
          disabled={installing}
        >
          ×
        </button>
      </div>
    </div>
  );
}

export function UpdateBanner(props: Props) {
  const version = props.updater.available?.version ?? null;
  const [expandedFor, setExpandedFor] = useState<string | null>(null);

  return (
    <UpdateBannerView
      {...props}
      expanded={version !== null && expandedFor === version}
      onToggleExpanded={() =>
        setExpandedFor((current) => (current === version ? null : version))
      }
    />
  );
}
