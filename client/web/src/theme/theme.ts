// Theme preference (design README: "Themes switch on [data-theme='dark']").
// The stylesheet knows only two states — the `dark` Tailwind variant keys on
// `[data-theme="dark"]` and nothing consults `prefers-color-scheme` — so
// "follow the system" has to be resolved here in JS and written onto the
// root element like any other explicit choice.

import type { StorageLike } from "../calendar/persistence";

const THEME_KEY = "hb.theme";

/** What the user chose. `"system"` is a preference, not a theme: it resolves
 * against the OS setting every time that setting changes. */
export type ThemePreference = "system" | "light" | "dark";

/** What actually gets written to `data-theme`. */
export type ResolvedTheme = "light" | "dark";

export function resolveTheme(
  preference: ThemePreference,
  systemPrefersDark: boolean,
): ResolvedTheme {
  if (preference === "system") {
    return systemPrefersDark ? "dark" : "light";
  }
  return preference;
}

/** The nav rail's sun/moon toggle. Flipping always lands on an explicit
 * preference — a toggle that could return to `"system"` would be a
 * three-state control wearing a two-state affordance. The Settings select is
 * the only way back to following the system. */
export function toggledPreference(resolved: ResolvedTheme): ThemePreference {
  return resolved === "dark" ? "light" : "dark";
}

export function readThemePreference(storage: StorageLike): ThemePreference {
  const raw = storage.getItem(THEME_KEY);
  return raw === "light" || raw === "dark" || raw === "system" ? raw : "system";
}

export function writeThemePreference(
  storage: StorageLike,
  preference: ThemePreference,
): void {
  if (preference === "system") {
    storage.removeItem(THEME_KEY);
  } else {
    storage.setItem(THEME_KEY, preference);
  }
}
