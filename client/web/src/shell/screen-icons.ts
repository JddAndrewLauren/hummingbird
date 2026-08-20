import type { IconName } from "../components/core/Icon";
import type { Screen } from "./screens";

/** Each screen's glyph, shared by both nav forms (`NavRail`, `NavBar`) so a
 * surface cannot be drawn with two different icons.
 *
 * Split from `SCREEN_LABELS` in `screens.ts` for one reason: this file imports
 * from `components/`, and `screens.ts` is compiled by `tsconfig.node.json` as
 * well (the visual gate's spec reads the labels to drive the phone's More
 * sheet), where there is no JSX. Keeping the icon here is what lets that
 * module stay component-free. */
export const SCREEN_ICONS: Record<Screen, IconName> = {
  now: "zap",
  triage: "inbox",
  projects: "folder-kanban",
  alerts: "bell",
  rules: "siren",
  done: "circle-check",
  ledger: "scroll-text",
  status: "activity",
  settings: "settings",
};
