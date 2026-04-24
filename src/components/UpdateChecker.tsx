import { openUrl } from "@tauri-apps/plugin-opener";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import type { AutoUpdate } from "../hooks/useAutoUpdate";

const RELEASES_URL = "https://github.com/ungurenko/parrot/releases/latest";

interface Props {
  updater: AutoUpdate;
}

export function UpdateChecker({ updater }: Props) {
  const {
    available,
    status,
    progress,
    errorDetails,
    errorScope,
    runCheck,
    install,
  } = updater;

  const label = (() => {
    if (status === "checking") return "Проверяю обновления…";
    if (status === "installing")
      return progress > 0 ? `Обновляю… ${progress}%` : "Готовлю обновление…";
    if (available)
      return available.version
        ? `Обновить до v${available.version}`
        : "Установить обновление";
    if (status === "current") return "У вас последняя версия";
    if (status === "error")
      return errorScope === "install"
        ? "Не удалось обновить"
        : "Не удалось проверить";
    return "Проверить обновления";
  })();

  const buttonAction = available ? install : () => runCheck(true);
  const disabled = status === "checking" || status === "installing";
  const shortDetails = errorDetails
    ? errorDetails.split("\n")[0].slice(0, 140)
    : null;

  const copyErrorDetails = async () => {
    if (!errorDetails) return;
    try {
      await navigator.clipboard.writeText(errorDetails);
      toast.success("Детали ошибки скопированы");
    } catch (e) {
      toast.error("Не удалось скопировать", { description: String(e) });
    }
  };

  return (
    <div className="flex flex-col gap-1 text-xs text-muted-foreground">
      <div className="flex items-center gap-2">
        <Button
          type="button"
          variant="link"
          size="sm"
          onClick={buttonAction}
          disabled={disabled}
          className="h-auto p-0 text-xs text-muted-foreground hover:text-foreground"
          title={status === "error" && errorDetails ? errorDetails : undefined}
        >
          {label}
        </Button>
        <span aria-hidden="true">·</span>
        <Button
          type="button"
          variant="link"
          size="sm"
          onClick={() => openUrl(RELEASES_URL)}
          className="h-auto p-0 text-xs text-muted-foreground hover:text-foreground"
        >
          Релизы
        </Button>
        {status === "error" && errorDetails && (
          <>
            <span aria-hidden="true">·</span>
            <Button
              type="button"
              variant="link"
              size="sm"
              onClick={copyErrorDetails}
              className="h-auto p-0 text-xs text-muted-foreground hover:text-foreground"
            >
              Скопировать детали
            </Button>
          </>
        )}
      </div>
      {status === "error" && shortDetails && (
        <span
          className="font-mono text-[10px] leading-tight text-muted-foreground/80"
          title={errorDetails ?? undefined}
        >
          {shortDetails}
        </span>
      )}
    </div>
  );
}
