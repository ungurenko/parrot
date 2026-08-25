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

interface Props {
  updater: AutoUpdate;
  onDismiss: () => void;
  onOpenSettings: () => void;
}

export function UpdateBanner({ updater, onDismiss, onOpenSettings }: Props) {
  const { available, status, progress, errorScope } = updater;
  const [expandedFor, setExpandedFor] = useState<string | null>(null);
  if (!available) return null;

  const installing = status === "installing";
  const installError = status === "error" && errorScope === "install";
  const notes = parseReleaseNotes(available.body);
  const expanded = expandedFor === available.version;

  return (
    <div className="update-banner" role="region" aria-label="Обновление Parrot">
      <div className="update-banner-icon" aria-hidden="true">
        <BellIcon size={15} />
      </div>
      <div className="update-banner-body">
        <div className="update-banner-title-row">
          <span className="update-banner-title">Обновление Parrot</span>
          <Badge variant="secondary">v{available.version}</Badge>
        </div>
        {installing ? (
          <Progress
            value={Math.max(progress, 2)}
            className="update-banner-progress"
          />
        ) : installError ? (
          <p className="update-banner-summary">
            Не удалось установить обновление.{" "}
            <button
              type="button"
              className="update-banner-subtitle underline underline-offset-2"
              onClick={onOpenSettings}
            >
              Подробнее — в Настройках
            </button>
          </p>
        ) : (
          <>
            {notes.summary && (
              <p className="update-banner-summary">{notes.summary}</p>
            )}
            {notes.details.length > 0 && (
              <>
                <div className="update-banner-meta">
                  Меньше минуты · Перезапустится автоматически
                  <button
                    type="button"
                    className="update-banner-notes-toggle"
                    onClick={() =>
                      setExpandedFor(expanded ? null : available.version)
                    }
                    aria-expanded={expanded}
                  >
                    Что нового
                    {expanded ? (
                      <ChevronUpIcon size={12} />
                    ) : (
                      <ChevronDownIcon size={12} />
                    )}
                  </button>
                </div>
                {expanded && (
                  <div className="update-banner-notes">
                    {notes.details.map((paragraph, idx) => (
                      <p key={idx}>{paragraph}</p>
                    ))}
                  </div>
                )}
              </>
            )}
          </>
        )}
      </div>
      <div className="update-banner-actions">
        <Button
          type="button"
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
