// The shell's surfaces (#107's decomposition). Switching is local state, not
// a router: there are eight screens, no deep links yet, and no URL contract to
// honour — adding a router would be a dependency carrying no weight.

export type Screen =
  | "now"
  | "triage"
  | "routes"
  | "alerts"
  | "rules"
  | "done"
  | "ledger"
  | "status"
  | "settings";

/** Rail order, defined once. `NavRail` maps over this and looks its labels and
 * icons up by screen, so the order lives here and nowhere else. The working
 * surfaces come first; Done, the Ledger and Status are the looking-at-things
 * (rather than acting-on-things) trio, so they sit between the working set
 * and Settings — Status last of the three (ADR-0017, #311) since it looks at
 * the machine rather than at the reader's own history. */
export const SCREENS: readonly Screen[] = [
  "now",
  "triage",
  "routes",
  "alerts",
  "rules",
  "done",
  "ledger",
  "status",
  "settings",
] as const;

/** Each screen's name in a navigation control, and its accessible name in
 * either nav form. Being a `Record<Screen, …>` it cannot silently miss one.
 *
 * Here rather than inside a nav component because there are two of them now —
 * `NavRail` on a desktop and `NavBar` on a phone — and a label that differed
 * between them would be one surface with two names. The *order* is `SCREENS`
 * above; this record only says what each one is called.
 *
 * The glyphs are `screen-icons.ts`, separately, so that this module stays free
 * of any import from `components/` — the visual gate's spec reads these labels
 * to drive the phone's More sheet, and it compiles under `tsconfig.node.json`,
 * which has no JSX. */
export const SCREEN_LABELS: Record<Screen, string> = {
  now: "Now",
  triage: "Triage",
  routes: "Routes",
  alerts: "Alerts",
  rules: "Rules",
  done: "Done",
  ledger: "Ledger",
  status: "Status",
  settings: "Settings",
};

/** The `<h1>` per screen. "Now" is the nav label; the header asks the
 * question the screen answers. */
export const SCREEN_TITLES: Record<Screen, string> = {
  now: "What's next",
  triage: "Triage",
  routes: "Routes",
  alerts: "Alerts",
  rules: "Rules",
  done: "Done",
  ledger: "Ledger",
  status: "Is everything working",
  settings: "Settings",
};
