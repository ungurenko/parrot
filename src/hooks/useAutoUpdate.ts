import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

const AUTO_CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;
const CURRENT_STATUS_TIMEOUT_MS = 3000;

export type AutoUpdateStatus =
  | "idle"
  | "checking"
  | "current"
  | "installing"
  | "error";

export type AutoUpdateScope = "check" | "install";

export interface AutoUpdate {
  available: Update | null;
  status: AutoUpdateStatus;
  progress: number;
  errorDetails: string | null;
  errorScope: AutoUpdateScope | null;
  runCheck: (manual: boolean) => Promise<void>;
  install: () => Promise<void>;
}

export function formatUpdateError(error: unknown): string {
  if (error instanceof Error) {
    const stack =
      error.stack && error.stack !== error.message ? error.stack : "";
    return stack ? `${error.message}\n${stack}` : error.message;
  }
  if (typeof error === "string") return error;
  try {
    return JSON.stringify(error);
  } catch {
    return String(error);
  }
}

/**
 * Single source-of-truth for the Tauri auto-updater. Call once at the top
 * of the component tree (App.tsx) and pass the result down to anyone who
 * wants to show update state (main-window banner, Settings footer).
 */
export function useAutoUpdate(): AutoUpdate {
  const [available, setAvailable] = useState<Update | null>(null);
  const [status, setStatus] = useState<AutoUpdateStatus>("idle");
  const [progress, setProgress] = useState(0);
  const [errorDetails, setErrorDetails] = useState<string | null>(null);
  const [errorScope, setErrorScope] = useState<AutoUpdateScope | null>(null);

  const currentTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const inFlightRef = useRef(false);

  const clearErrors = useCallback(() => {
    setErrorDetails(null);
    setErrorScope(null);
  }, []);

  const reportError = useCallback(
    (scope: AutoUpdateScope, error: unknown) => {
      const message = formatUpdateError(error);
      setErrorDetails(message);
      setErrorScope(scope);
      invoke("log_client_error", { scope: `updater:${scope}`, message }).catch(
        (e) => console.error("log_client_error failed:", e),
      );
    },
    [],
  );

  const runCheck = useCallback(
    async (manual: boolean) => {
      if (inFlightRef.current) return;
      if (status === "installing") return;
      inFlightRef.current = true;

      if (manual) {
        clearErrors();
        setStatus("checking");
      }

      try {
        const update = await check();
        if (update) {
          setAvailable(update);
          clearErrors();
          // Переход в "idle" — кнопка-действие читает `available` напрямую.
          setStatus("idle");
        } else {
          setAvailable(null);
          clearErrors();
          if (manual) {
            setStatus("current");
            if (currentTimerRef.current) clearTimeout(currentTimerRef.current);
            currentTimerRef.current = setTimeout(
              () => setStatus("idle"),
              CURRENT_STATUS_TIMEOUT_MS,
            );
          } else {
            setStatus("idle");
          }
        }
      } catch (error) {
        console.error("Auto-update check failed:", error);
        reportError("check", error);
        if (manual) setStatus("error");
        else setStatus("idle");
      } finally {
        inFlightRef.current = false;
      }
    },
    [status, clearErrors, reportError],
  );

  const install = useCallback(async () => {
    if (status === "installing") return;
    try {
      setStatus("installing");
      setProgress(0);
      clearErrors();
      const update = available ?? (await check());
      if (!update) {
        setStatus("idle");
        setAvailable(null);
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
            setProgress(
              Math.min(100, Math.round((downloaded / contentLength) * 100)),
            );
          }
        }
      });
      await relaunch();
    } catch (error) {
      console.error("Auto-update install failed:", error);
      reportError("install", error);
      setStatus("error");
    }
  }, [status, available, clearErrors, reportError]);

  useEffect(() => {
    void runCheck(false);
    const handle = setInterval(
      () => void runCheck(false),
      AUTO_CHECK_INTERVAL_MS,
    );
    return () => {
      clearInterval(handle);
      if (currentTimerRef.current) clearTimeout(currentTimerRef.current);
    };
    // Запускаем один раз при монтировании App. runCheck содержит замыкание
    // над актуальным state через функциональные setState, повторный запуск
    // цикла на изменение зависимостей нежелателен.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return {
    available,
    status,
    progress,
    errorDetails,
    errorScope,
    runCheck,
    install,
  };
}
