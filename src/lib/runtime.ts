import {
  listen,
  type Event,
  type EventCallback,
  type EventName,
  type UnlistenFn,
} from "@tauri-apps/api/event";

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

export type TauriEventPayload<T> = Event<T>;
