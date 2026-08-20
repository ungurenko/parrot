import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { cleanupTauriListeners, isTauriRuntime, listenInTauri } from "@/lib/runtime";
import type { HistoryEntry, LoadedHistoryEntry } from "../types";

export function useHistory() {
  const [history, setHistory] = useState<HistoryEntry[]>([]);

  const reload = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      const list = await invoke<HistoryEntry[]>("get_history");
      setHistory(list);
    } catch (e) {
      console.error("get_history failed:", e);
    }
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    reload();
    const listeners = [
      listenInTauri("history:updated", () => {
        reload();
      }),
    ];
    return () => {
      cleanupTauriListeners(listeners);
    };
  }, [reload]);

  const deleteEntry = useCallback(
    async (id: string) => {
      if (!isTauriRuntime()) return;
      try {
        await invoke("delete_history_entry", { id });
        // Optimistic local update; backend also emits history:updated.
        setHistory((prev) => prev.filter((e) => e.id !== id));
      } catch (e) {
        console.error("delete_history_entry failed:", e);
      }
    },
    [],
  );

  const clearAll = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      await invoke("clear_history");
      setHistory([]);
    } catch (e) {
      console.error("clear_history failed:", e);
    }
  }, []);

  const loadEntry = useCallback(
    async (id: string): Promise<LoadedHistoryEntry | null> => {
      if (!isTauriRuntime()) return null;
      try {
        return await invoke<LoadedHistoryEntry>("load_history_entry", { id });
      } catch (e) {
        console.error("load_history_entry failed:", e);
        throw e;
      }
    },
    [],
  );

  return { history, deleteEntry, clearAll, loadEntry };
}
