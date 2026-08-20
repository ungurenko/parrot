import { useEffect, useState } from "react";
import type { Theme } from "../types";

export type ResolvedTheme = "light" | "dark";

export const THEME_STORAGE_KEY = "parrot-theme";

/**
 * Resolve the effective (light/dark) theme from the user preference.
 * When `theme === "system"`, fall back to the OS preference.
 */
function resolveTheme(
  theme: Theme | undefined,
  prefersDark: boolean,
): ResolvedTheme {
  switch (theme) {
    case "light":
    case "dark":
      return theme;
    case "system":
    default:
      return prefersDark ? "dark" : "light";
  }
}

/**
 * Apply the theme to the document and persist the resolved choice so the
 * inline pre-render script in index.html can avoid a flash of the wrong theme.
 */
function applyTheme(resolved: ResolvedTheme) {
  const root = document.documentElement;
  root.classList.toggle("dark", resolved === "dark");
  try {
    localStorage.setItem(THEME_STORAGE_KEY, resolved);
  } catch {
    /* localStorage unavailable — ignore */
  }
}

/**
 * React to the user's theme preference. Toggles the `.dark` class on
 * `<html>` and, in "system" mode, follows live changes to the macOS
 * appearance via the `prefers-color-scheme` media query.
 */
export function useTheme(theme: Theme | undefined): ResolvedTheme {
  const [resolved, setResolved] = useState<ResolvedTheme>(() => {
    const prefersDark = window.matchMedia(
      "(prefers-color-scheme: dark)",
    ).matches;
    return resolveTheme(theme, prefersDark);
  });

  useEffect(() => {
    const mql = window.matchMedia("(prefers-color-scheme: dark)");
    const prefersDark = mql.matches;
    const next = resolveTheme(theme, prefersDark);
    setResolved(next);
    applyTheme(next);

    if (theme !== "system") return;
    const onChange = (e: MediaQueryListEvent) => {
      const updated = resolveTheme("system", e.matches);
      setResolved(updated);
      applyTheme(updated);
    };
    mql.addEventListener("change", onChange);
    return () => mql.removeEventListener("change", onChange);
  }, [theme]);

  return resolved;
}
