import { useEffect, useState } from "react";
import type { Theme } from "../types";

export type ResolvedTheme = "light" | "dark";

const THEME_STORAGE_KEY = "parrot-theme";

const prefersDarkQuery = () => window.matchMedia("(prefers-color-scheme: dark)");

/**
 * Resolve the effective (light/dark) theme from the user preference.
 * When `theme === "system"`, fall back to the OS preference.
 */
export function resolveTheme(
  theme: Theme,
  prefersDark: boolean,
): ResolvedTheme {
  switch (theme) {
    case "light":
    case "dark":
      return theme;
    case "system":
      return prefersDark ? "dark" : "light";
  }
}

/**
 * Apply a resolved theme to the document. Never touches localStorage — the
 * persisted value is the user's preference, written by `useTheme`.
 */
function applyResolved(resolved: ResolvedTheme) {
  document.documentElement.classList.toggle("dark", resolved === "dark");
}

/**
 * Persist the user's theme *preference* (system/light/dark) so the inline
 * pre-render script in index.html can avoid a flash of the wrong theme.
 */
function persistPreference(theme: Theme) {
  try {
    localStorage.setItem(THEME_STORAGE_KEY, theme);
  } catch {
    /* localStorage unavailable — ignore */
  }
}

/**
 * React to the user's theme preference. Toggles the `.dark` class on
 * `<html>` and, in "system" mode, follows live changes to the macOS
 * appearance via the `prefers-color-scheme` media query.
 *
 * The initial resolved theme is read from the DOM — the inline script in
 * index.html has already applied the persisted/OS theme before React mounts —
 * so there is no flash or fight over the `.dark` class while settings load.
 * While `theme` is still undefined (settings not loaded yet) this hook does
 * nothing and trusts that pre-render state.
 */
export function useTheme(theme: Theme | undefined): ResolvedTheme {
  const [resolved, setResolved] = useState<ResolvedTheme>(() =>
    document.documentElement.classList.contains("dark") ? "dark" : "light",
  );

  useEffect(() => {
    if (theme === undefined) return;

    const apply = (prefersDark: boolean) => {
      const next = resolveTheme(theme, prefersDark);
      setResolved(next);
      applyResolved(next);
    };

    apply(prefersDarkQuery().matches);
    persistPreference(theme);

    if (theme !== "system") return;
    const mql = prefersDarkQuery();
    const onChange = (e: MediaQueryListEvent) => apply(e.matches);
    mql.addEventListener("change", onChange);
    return () => mql.removeEventListener("change", onChange);
  }, [theme]);

  return resolved;
}
