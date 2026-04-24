import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import type { AutoUpdate } from "../hooks/useAutoUpdate";

interface Props {
  updater: AutoUpdate;
  onDismiss: () => void;
  onOpenSettings: () => void;
}

export function UpdateBanner({ updater, onDismiss, onOpenSettings }: Props) {
  const { available, status, progress, errorScope } = updater;
  if (!available) return null;

  const installing = status === "installing";
  const installError = status === "error" && errorScope === "install";

  const buttonLabel = installing
    ? progress > 0
      ? `Обновляю… ${progress}%`
      : "Готовлю…"
    : `⬆︎ Обновить до v${available.version}`;

  return (
    <div className="update-banner" role="region" aria-label="Обновление Parrot">
      <div className="update-banner-icon" aria-hidden="true">
        🔔
      </div>
      <div className="update-banner-body">
        <div className="update-banner-title">
          Доступна новая версия Parrot v{available.version}
        </div>
        {installing ? (
          <Progress
            value={Math.max(progress, 2)}
            className="update-banner-progress"
          />
        ) : installError ? (
          <button
            type="button"
            className="update-banner-subtitle underline underline-offset-2"
            onClick={onOpenSettings}
          >
            Ошибка установки — открыть Настройки с деталями
          </button>
        ) : (
          <div className="update-banner-subtitle">
            Займёт меньше минуты. Parrot перезапустится автоматически.
          </div>
        )}
      </div>
      <div className="update-banner-actions">
        {!installError && (
          <Button
            type="button"
            size="sm"
            onClick={updater.install}
            disabled={installing}
          >
            {buttonLabel}
          </Button>
        )}
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
