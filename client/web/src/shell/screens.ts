// The shell's surfaces (#107's decomposition). Switching is local state, not
// a router: there are seven screens, no deep links yet, and no URL contract to
// honour — adding a router would be a dependency carrying no weight.

export type Screen = "now" | "triage" | "routes" | "alerts" | "done" | "ledger" | "settings";

/** Rail order, defined once. `NavRail` maps over this and looks its labels and
 * icons up by screen, so the order lives here and nowhere else. The working
 * surfaces come first; Done and the Ledger are the looking-back pair, so they
 * sit between the working set and Settings. */
export const SCREENS: readonly Screen[] = [
  "now",
  "triage",
  "routes",
  "alerts",
  "done",
  "ledger",
  "settings",
] as const;

/** The `<h1>` per screen. "Now" is the nav label; the header asks the
 * question the screen answers. */
export const SCREEN_TITLES: Record<Screen, string> = {
  now: "What's next",
  triage: "Triage",
  routes: "Routes",
  alerts: "Alerts",
  done: "Done",
  ledger: "Ledger",
  settings: "Settings",
};
