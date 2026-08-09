import { useCallback, useEffect, useState, useSyncExternalStore } from "react";
import {
  readThemePreference,
  resolveTheme,
  writeThemePreference,
  type ResolvedTheme,
  type ThemePreference,
} from "./theme";

const DARK_QUERY = "(prefers-color-scheme: dark)";

function systemPrefersDark(): boolean {
  return window.matchMedia(DARK_QUERY).matches;
}

// The OS setting is external state, so it is read through
// `useSyncExternalStore` rather than mirrored into React state. Copying it
// into `useState` meant re-syncing inside an effect, which renders once with
// the stale value before correcting itself — a visible wrong-theme frame when
// returning to "follow system". Reading it here is always fresh.
function subscribeToSystemTheme(onStoreChange: () => void): () => void {
  const query = window.matchMedia(DARK_QUERY);
  query.addEventListener("change", onStoreChange);
  return () => query.removeEventListener("change", onStoreChange);
}

function applyTheme(theme: ResolvedTheme): void {
  document.documentElement.setAttribute("data-theme", theme);
}

/** Writes the resolved theme onto the root element before React mounts, so
 * the first paint is already in the right theme. This has to be JS rather
 * than an inline `<script>` in index.html: the production CSP is
 * `script-src 'self'`, which allows no inline script. */
export function applyInitialTheme(): void {
  applyTheme(resolveTheme(readThemePreference(localStorage), systemPrefersDark()));
}

export interface ThemeControl {
  preference: ThemePreference;
  theme: ResolvedTheme;
  setPreference: (preference: ThemePreference) => void;
}

export function useTheme(): ThemeControl {
  const [preference, setPreferenceState] = useState<ThemePreference>(() =>
    readThemePreference(localStorage),
  );
  const prefersDark = useSyncExternalStore(subscribeToSystemTheme, systemPrefersDark);

  // The subscription is unconditional, but an explicit light/dark preference
  // simply ignores the value — `resolveTheme` only consults it under
  // "system", so a system flip at sunset cannot move an explicit choice.
  const theme = resolveTheme(preference, prefersDark);

  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  const setPreference = useCallback((next: ThemePreference) => {
    writeThemePreference(localStorage, next);
    setPreferenceState(next);
  }, []);

  return { preference, theme, setPreference };
}
