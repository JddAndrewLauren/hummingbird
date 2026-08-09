import { useCallback, useEffect, useState } from "react";
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
  const [prefersDark, setPrefersDark] = useState<boolean>(systemPrefersDark);

  // Only subscribed while the preference actually defers to the OS. An
  // explicit light/dark choice must not move when the system flips at
  // sunset, so there is nothing to listen for in that state.
  useEffect(() => {
    if (preference !== "system") {
      return;
    }
    const query = window.matchMedia(DARK_QUERY);
    const onChange = (event: MediaQueryListEvent) => setPrefersDark(event.matches);
    setPrefersDark(query.matches);
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, [preference]);

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
