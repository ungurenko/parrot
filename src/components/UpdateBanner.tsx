import { useState } from "react";
import { ArrowUpIcon, BellIcon, XIcon } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Progress } from "@/components/ui/progress";
import { Separator } from "@/components/ui/separator";
import type { AutoUpdate } from "../hooks/useAutoUpdate";
import { parseReleaseNotes } from "@/lib/releaseNotes";

interface Props {
  updater: AutoUpdate;
  onDismiss: () => void;
  onOpenSettings: () => void;
}

interface UpdateBannerViewProps extends Props {
  notesOpen: boolean;
  onNotesOpenChange: (open: boolean) => void;
}

interface UpdateNotesContentProps {
  highlights: string[];
  onInstall: () => void;
}

export function UpdateNotesContent({
  highlights,
  onInstall,
}: UpdateNotesContentProps) {
  return (
    <>
      <ul className="update-notes-list">
        {highlights.map((highlight, index) => (
          <li className="update-notes-item" key={`${index}-${highlight}`}>
            <span className="update-notes-dot" aria-hidden="true" />
            <span>{highlight}</span>
          </li>
        ))}
      </ul>
      <Separator />
      <div className="update-notes-footer">
        <span className="update-notes-meta">
          Меньше минуты · Parrot перезапустится сам
        </span>
        <Button type="button" size="sm" onClick={onInstall}>
          <ArrowUpIcon data-icon="inline-start" />
          Обновить
        </Button>
      </div>
    </>
  );
}

export function UpdateBannerView({
  updater,
  notesOpen,
  onNotesOpenChange,
  onDismiss,
  onOpenSettings,
}: UpdateBannerViewProps) {
  const { available, status, progress, errorScope } = updater;
  if (!available) return null;

  const installing = status === "installing";
  const installError = status === "error" && errorScope === "install";
  const notes = parseReleaseNotes(available.body);
  const hasNotes = notes.highlights.length > 0;
  const dialogOpen = notesOpen && hasNotes && !installing && !installError;
  const title = installing
    ? "Обновляю Parrot"
    : installError
      ? "Обновление не установлено"
      : "Доступна новая версия";

  const install = () => {
    onNotesOpenChange(false);
    void updater.install();
  };

  return (
    <>
      <div
        className="update-banner mx-4 h-16 px-3 py-2"
        role="region"
        aria-label="Обновление Parrot"
      >
        <div className="update-banner-icon" aria-hidden="true">
          <BellIcon size={15} />
        </div>
        <div className="update-banner-body">
          <div className="update-banner-title-row">
            <span className="update-banner-title">{title}</span>
            <Badge variant="secondary">v{available.version}</Badge>
          </div>
          {installing ? (
            <Progress
              value={Math.max(progress, 2)}
              className="update-banner-progress"
            />
          ) : (
            <p className="update-banner-summary">
              {installError
                ? "Попробуйте ещё раз или откройте подробности."
                : notes.summary || "Обновление готово к установке."}
            </p>
          )}
        </div>
        <div className="update-banner-actions">
          {installError && (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={onOpenSettings}
            >
              Подробнее
            </Button>
          )}
          {!installing && !installError && hasNotes && (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={() => onNotesOpenChange(true)}
              aria-haspopup="dialog"
              aria-expanded={notesOpen}
              aria-controls="update-notes-dialog"
            >
              Что нового
            </Button>
          )}
          <Button
            type="button"
            size="sm"
            onClick={install}
            disabled={installing}
          >
            <ArrowUpIcon data-icon="inline-start" />
            {installing
              ? progress > 0
                ? `Обновляю… ${progress}%`
                : "Готовлю…"
              : installError
                ? "Повторить"
                : "Обновить"}
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            onClick={onDismiss}
            aria-label="Скрыть подсказку"
            title="Скрыть до следующего запуска"
            disabled={installing}
          >
            <XIcon />
          </Button>
        </div>
      </div>

      <Dialog open={dialogOpen} onOpenChange={onNotesOpenChange}>
        <DialogContent id="update-notes-dialog" className="sm:max-w-md">
          <DialogHeader>
            <div className="update-notes-title-row">
              <DialogTitle>Что нового в Parrot</DialogTitle>
              <Badge variant="secondary">v{available.version}</Badge>
            </div>
            <DialogDescription>Главное в новой версии.</DialogDescription>
          </DialogHeader>
          <UpdateNotesContent
            highlights={notes.highlights}
            onInstall={install}
          />
        </DialogContent>
      </Dialog>
    </>
  );
}

export function UpdateBanner(props: Props) {
  const version = props.updater.available?.version ?? null;
  const [notesOpenFor, setNotesOpenFor] = useState<string | null>(null);

  return (
    <UpdateBannerView
      {...props}
      notesOpen={version !== null && notesOpenFor === version}
      onNotesOpenChange={(open) => setNotesOpenFor(open ? version : null)}
    />
  );
}
