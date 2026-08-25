import {
  listen,
  type EventCallback,
  type EventName,
  type UnlistenFn,
} from "@tauri-apps/api/event";
import { DEFAULT_SUMMARY_MODEL, type Settings } from "../types";

export function previewSettings(saveDir: string): Settings {
  return {
    save_dir: saveDir,
    onboarded: true,
    engine: "parakeet",
    language: "ru",
    summarizer_enabled: false,
    summary_model: DEFAULT_SUMMARY_MODEL,
    summarizer_promo_seen: true,
    dictation_enabled: true,
    dictation_hold_key: "Alt+Space",
    theme: "system",
  };
}

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function listenInTauri<T>(
  event: EventName,
  handler: EventCallback<T>,
): Promise<UnlistenFn | null> {
  if (!isTauriRuntime()) return Promise.resolve(null);

  return listen<T>(event, handler).catch((error) => {
    if (isTauriRuntime()) {
      console.error(`Failed to listen to ${event}:`, error);
    }
    return null;
  });
}

export function cleanupTauriListeners(
  listeners: Array<Promise<UnlistenFn | null>>,
): void {
  listeners.forEach((listener) => {
    listener
      .then((unlisten) => {
        unlisten?.();
      })
      .catch(() => {});
  });
}
