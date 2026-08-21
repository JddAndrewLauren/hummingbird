// The frontier board's view preferences — the chosen grouping axis and the set
// of collapsed columns (#403, ADR-0021 decision 5). A view preference on one
// device, not a cross-device fact, so it lives in the injectable-`storage`
// idiom (`screens/storage.ts`'s `StorageLike`) `shell/rail-collapse.ts` and
// `screens/questions/collapse.ts` already use — never in the `settings` table,
// which has no DELETE and syncs everywhere, so an axis or a column-label map
// would accrete keys forever and follow the reader onto devices whose widths
// make it wrong.
//
// Every call tolerates a broken or absent storage (private-mode Safari throws
// on `setItem`; a worker context has none): a preference that cannot persist
// still applies for the session, it just resets on reload.
//
// **What is deliberately NOT here: the facet filter selection.** You must never
// open Now to a filtered set of columns and misread it as an empty frontier — a
// remembered filter is a remembered lie about what you have to do. It is
// component state in `FrontierColumns.tsx` and stays there.
//
// **Keyed per screen, not per board instance.** The board is on two surfaces
// now — Now, and one project's dossier — and each gets its own key namespace
// (`hb.now.*` / `hb.projects.*`) so choosing "size" on a project cannot
// re-group Now behind your back. It stops there: the projects keys are shared
// by *every* project, not one pair per project. That matters only for the
// collapsed set, because the prune-against-`liveKeys` write in
// `FrontierColumns.tsx` discards entries no column on the current board can
// claim — so opening project B can drop project A's collapsed columns.
// Accepted: an axis is the preference worth remembering, and per-project
// collapse entries are exactly the unbounded key accretion ADR-0021
// decision 5 keeps out of the `settings` table.

import {
  DEFAULT_FRONTIER_AXIS,
  FRONTIER_AXES,
  type FrontierAxis,
} from "./frontier-columns";
import type { StorageLike } from "./storage";

/** Which board's preferences to read or write. Not a free string: the two
 * surfaces that mount `FrontierColumns` are the whole population, and a typo'd
 * namespace would silently give a screen a private, permanently-default set of
 * preferences. */
export type FrontierPrefsScreen = "now" | "projects";

function axisKey(screen: FrontierPrefsScreen): string {
  return `hb.${screen}.frontier-axis`;
}

function collapsedKey(screen: FrontierPrefsScreen): string {
  return `hb.${screen}.frontier-collapsed`;
}

/** The axis last chosen on this device, or the default. An unrecognised
 * stored value degrades to the default rather than erroring — a newer build's
 * vocabulary, or a hand-edited key, and the default rule is always a correct
 * answer.
 *
 * `allowedAxes` is that same rule applied to a surface that renders only a
 * subset of the vocabulary (the project board drops the degenerate `project`
 * axis): a stored axis this board cannot switch back to is as unusable as an
 * unrecognised one, so it degrades the same way rather than grouping by an
 * axis whose button is not on screen. */
export function readFrontierAxis(
  storage: StorageLike | undefined,
  screen: FrontierPrefsScreen,
  allowedAxes: readonly FrontierAxis[] = FRONTIER_AXES,
): FrontierAxis {
  if (!storage) {
    return DEFAULT_FRONTIER_AXIS;
  }
  try {
    const stored = storage.getItem(axisKey(screen));
    return allowedAxes.find((axis) => axis === stored) ?? DEFAULT_FRONTIER_AXIS;
  } catch {
    return DEFAULT_FRONTIER_AXIS;
  }
}

export function writeFrontierAxis(
  storage: StorageLike | undefined,
  screen: FrontierPrefsScreen,
  axis: FrontierAxis,
): void {
  if (!storage) {
    return;
  }
  try {
    if (axis === DEFAULT_FRONTIER_AXIS) {
      // The default is encoded as key *absence*, never as a stored value, so
      // it cannot rot into a stale legacy default when the default changes.
      storage.removeItem(axisKey(screen));
    } else {
      storage.setItem(axisKey(screen), axis);
    }
  } catch {
    // Session-only preference; nothing to do.
  }
}

/** The columns shut on this device, keyed by the column's own label. Anything
 * unparseable — or parseable but not an array of strings — reads as "none
 * collapsed", the default. */
export function readCollapsedColumns(
  storage: StorageLike | undefined,
  screen: FrontierPrefsScreen,
): ReadonlySet<string> {
  if (!storage) {
    return new Set();
  }
  try {
    const stored = storage.getItem(collapsedKey(screen));
    if (stored === null) {
      return new Set();
    }
    const parsed: unknown = JSON.parse(stored);
    if (!Array.isArray(parsed) || parsed.some((entry) => typeof entry !== "string")) {
      return new Set();
    }
    return new Set(parsed as string[]);
  } catch {
    return new Set();
  }
}

export function writeCollapsedColumns(
  storage: StorageLike | undefined,
  screen: FrontierPrefsScreen,
  collapsed: ReadonlySet<string>,
): void {
  if (!storage) {
    return;
  }
  try {
    if (collapsed.size === 0) {
      // Nothing collapsed is the default — an absent key says the same thing.
      storage.removeItem(collapsedKey(screen));
    } else {
      storage.setItem(collapsedKey(screen), JSON.stringify([...collapsed]));
    }
  } catch {
    // Session-only preference; nothing to do.
  }
}
