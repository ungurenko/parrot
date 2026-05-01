import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { HistoryEntry, LoadedHistoryEntry } from "../types";

export function useHistory() {
  const [history, setHistory] = useState<HistoryEntry[]>([]);

  const reload = useCallback(async () => {
    try {
      const list = await invoke<HistoryEntry[]>("get_history");
      setHistory(list);
    } catch (e) {
      console.error("get_history failed:", e);
    }
  }, []);

  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    reload();
    let unlisten: (() => void) | null = null;
    listen("history:updated", () => {
      reload();
    }).then((u) => {
      unlisten = u;
    });
    return () => {
      if (unlisten) unlisten();
    };
  }, [reload]);

  const deleteEntry = useCallback(
    async (id: string) => {
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
    if (!("__TAURI_INTERNALS__" in window)) return;
    try {
      await invoke("clear_history");
      setHistory([]);
    } catch (e) {
      console.error("clear_history failed:", e);
    }
  }, []);

  const loadEntry = useCallback(
    async (id: string): Promise<LoadedHistoryEntry | null> => {
      try {
        return await invoke<LoadedHistoryEntry>("load_history_entry", { id });
      } catch (e) {
        console.error("load_history_entry failed:", e);
        throw e;
      }
    },
    [],
  );

  return { history, reload, deleteEntry, clearAll, loadEntry };
}
