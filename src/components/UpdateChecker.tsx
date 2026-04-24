import { useEffect, useRef, useState } from "react";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { openUrl } from "@tauri-apps/plugin-opener";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";

const RELEASES_URL = "https://github.com/ungurenko/parrot/releases/latest";

type UpdateStatus =
  | "idle"
  | "checking"
  | "available"
  | "current"
  | "installing"
  | "error";

function formatError(error: unknown): string {
  if (error instanceof Error) {
    const stack = error.stack && error.stack !== error.message ? error.stack : "";
    return stack ? `${error.message}\n${stack}` : error.message;
  }
  if (typeof error === "string") return error;
  try {
    return JSON.stringify(error);
  } catch {
    return String(error);
  }
}

export function UpdateChecker() {
  const [status, setStatus] = useState<UpdateStatus>("idle");
  const [version, setVersion] = useState<string | null>(null);
  const [progress, setProgress] = useState(0);
  const [errorDetails, setErrorDetails] = useState<string | null>(null);
  const [errorScope, setErrorScope] = useState<"check" | "install" | null>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const updateRef = useRef<Awaited<ReturnType<typeof check>> | null>(null);

  useEffect(() => {
    void checkForUpdates(false);
    return () => {
      if (timerRef.current) {
        clearTimeout(timerRef.current);
      }
    };
  }, []);

  const showTemporaryStatus = (nextStatus: UpdateStatus) => {
    setStatus(nextStatus);
    if (timerRef.current) {
      clearTimeout(timerRef.current);
    }
    // Error status больше не сбрасывается автоматически — пользователь
    // должен успеть скопировать детали.
    if (nextStatus === "error") return;
    timerRef.current = setTimeout(() => setStatus("idle"), 3000);
  };

  const reportError = (scope: "check" | "install", error: unknown) => {
    const message = formatError(error);
    setErrorDetails(message);
    setErrorScope(scope);
    invoke("log_client_error", {
      scope: `updater:${scope}`,
      message,
    }).catch((e) => console.error("log_client_error failed:", e));
  };

  const checkForUpdates = async (manual: boolean) => {
    if (status === "checking" || status === "installing") return;

    if (manual) {
      setStatus("checking");
      setErrorDetails(null);
      setErrorScope(null);
    }

    try {
      const update = await check();
      updateRef.current = update;
      if (update) {
        setVersion(update.version);
        setErrorDetails(null);
        setErrorScope(null);
        setStatus("available");
      } else if (manual) {
        setErrorDetails(null);
        setErrorScope(null);
        showTemporaryStatus("current");
      }
    } catch (error) {
      console.error("Failed to check for updates:", error);
      reportError("check", error);
      if (manual) {
        showTemporaryStatus("error");
      }
    }
  };

  const installUpdate = async () => {
    if (status === "installing") return;

    try {
      setStatus("installing");
      setProgress(0);
      setErrorDetails(null);
      setErrorScope(null);
      const update = updateRef.current ?? (await check());
      if (!update) {
        showTemporaryStatus("current");
        return;
      }

      let downloaded = 0;
      let contentLength = 0;
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          downloaded = 0;
          contentLength = event.data.contentLength ?? 0;
          setProgress(0);
        }
        if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          if (contentLength > 0) {
            setProgress(Math.min(100, Math.round((downloaded / contentLength) * 100)));
          }
        }
      });
      await relaunch();
    } catch (error) {
      console.error("Failed to install update:", error);
      reportError("install", error);
      showTemporaryStatus("error");
    }
  };

  const copyErrorDetails = async () => {
    if (!errorDetails) return;
    try {
      await navigator.clipboard.writeText(errorDetails);
      toast.success("Детали ошибки скопированы");
    } catch (e) {
      toast.error("Не удалось скопировать", { description: String(e) });
    }
  };

  const label = (() => {
    if (status === "checking") return "Проверяю обновления…";
    if (status === "available") return version ? `Обновить до v${version}` : "Установить обновление";
    if (status === "installing") return progress > 0 ? `Обновляю… ${progress}%` : "Готовлю обновление…";
    if (status === "current") return "У вас последняя версия";
    if (status === "error")
      return errorScope === "install"
        ? "Не удалось обновить"
        : "Не удалось проверить";
    return "Проверить обновления";
  })();

  const buttonAction = status === "available" ? installUpdate : () => checkForUpdates(true);
  const disabled = status === "checking" || status === "installing";
  const shortDetails = errorDetails
    ? errorDetails.split("\n")[0].slice(0, 140)
    : null;

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
