/**
 * Theme — "Technical Journal" (light) / "Observatory" (dark).
 *
 * The active theme is persisted in localStorage under STORAGE_KEY and
 * applied as `document.documentElement.dataset.theme`.  The HTML element
 * ships with data-theme="dark" so there is no flash before JS runs.
 */

// ---- React hook -----------------------------------------------------------
import { useCallback, useState } from "react";

export type Theme = "dark" | "light";

const STORAGE_KEY = "i18n-harness-theme";
const ROOT = () => document.documentElement;

/** Read persisted theme; fall back to "dark" if absent or invalid. */
export function getTheme(): Theme {
  const stored = localStorage.getItem(STORAGE_KEY);
  return stored === "light" ? "light" : "dark";
}

/** Apply theme to <html> and persist to localStorage. */
export function setTheme(theme: Theme): void {
  ROOT().dataset.theme = theme;
  localStorage.setItem(STORAGE_KEY, theme);
  // Keep color-scheme in sync so scrollbars / system chrome follow suit.
  ROOT().style.colorScheme = theme;
}

/** Read from localStorage and immediately apply to the DOM.
 *  The lazy useState initializer calls this once on first render — no
 *  separate useEffect needed, so there is nothing to exhaust. */
function initAndGetTheme(): Theme {
  const t = getTheme();
  setTheme(t);
  return t;
}

export function useTheme(): [Theme, (t: Theme) => void] {
  // initAndGetTheme runs once on mount via the lazy initializer;
  // subsequent calls go through apply() which keeps DOM and state in sync.
  const [theme, setLocal] = useState<Theme>(initAndGetTheme);

  const apply = useCallback((next: Theme) => {
    setTheme(next);
    setLocal(next);
  }, []);

  return [theme, apply];
}
