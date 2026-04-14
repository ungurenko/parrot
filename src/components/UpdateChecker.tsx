import { useEffect, useRef, useState } from "react";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { openUrl } from "@tauri-apps/plugin-opener";

const RELEASES_URL = "https://github.com/ungurenko/parrot/releases/latest";

type UpdateStatus =
  | "idle"
  | "checking"
  | "available"
  | "current"
  | "installing"
  | "error";

export function UpdateChecker() {
  const [status, setStatus] = useState<UpdateStatus>("idle");
  const [version, setVersion] = useState<string | null>(null);
  const [progress, setProgress] = useState(0);
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
    timerRef.current = setTimeout(() => setStatus("idle"), 3000);
  };

  const checkForUpdates = async (manual: boolean) => {
    if (status === "checking" || status === "installing") return;

    if (manual) {
      setStatus("checking");
    }

    try {
      const update = await check();
      updateRef.current = update;
      if (update) {
        setVersion(update.version);
        setStatus("available");
      } else if (manual) {
        showTemporaryStatus("current");
      }
    } catch (error) {
      console.error("Failed to check for updates:", error);
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
      showTemporaryStatus("error");
    }
  };

  const label = (() => {
    if (status === "checking") return "Проверяю обновления…";
    if (status === "available") return version ? `Обновить до v${version}` : "Установить обновление";
    if (status === "installing") return progress > 0 ? `Обновляю… ${progress}%` : "Готовлю обновление…";
    if (status === "current") return "У вас последняя версия";
    if (status === "error") return "Не удалось проверить";
    return "Проверить обновления";
  })();

  const buttonAction = status === "available" ? installUpdate : () => checkForUpdates(true);
  const disabled = status === "checking" || status === "installing";

  return (
    <div className="flex items-center gap-2 text-xs text-[var(--color-muted)]">
      <button
        onClick={buttonAction}
        disabled={disabled}
        className="hover:text-[var(--color-text)] disabled:opacity-60"
      >
        {label}
      </button>
      <span>·</span>
      <button
        onClick={() => openUrl(RELEASES_URL)}
        className="hover:text-[var(--color-text)]"
      >
        Релизы
      </button>
    </div>
  );
}
