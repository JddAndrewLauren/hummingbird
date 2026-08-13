// Now's frontier view preferences — the chosen grouping axis and the set of
// collapsed columns (#403, ADR-0021 decision 5). A view preference on one
// device, not a cross-device fact, so it lives in the injectable-`storage`
// idiom `screens/triage-collapse.ts`, `shell/rail-collapse.ts` and
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

import {
  DEFAULT_FRONTIER_AXIS,
  FRONTIER_AXES,
  type FrontierAxis,
} from "./frontier-columns";
import type { StorageLike } from "./triage-collapse";

const AXIS_KEY = "hb.now.frontier-axis";
const COLLAPSED_KEY = "hb.now.frontier-collapsed";

/** The axis last chosen on this device, or the default. An unrecognised
 * stored value degrades to the default rather than erroring — a newer build's
 * vocabulary, or a hand-edited key, and the default rule is always a correct
 * answer. */
export function readFrontierAxis(storage: StorageLike | undefined): FrontierAxis {
  if (!storage) {
    return DEFAULT_FRONTIER_AXIS;
  }
  try {
    const stored = storage.getItem(AXIS_KEY);
    return FRONTIER_AXES.find((axis) => axis === stored) ?? DEFAULT_FRONTIER_AXIS;
  } catch {
    return DEFAULT_FRONTIER_AXIS;
  }
}

export function writeFrontierAxis(
  storage: StorageLike | undefined,
  axis: FrontierAxis,
): void {
  if (!storage) {
    return;
  }
  try {
    if (axis === DEFAULT_FRONTIER_AXIS) {
      // The default is encoded as key *absence*, never as a stored value, so
      // it cannot rot into a stale legacy default when the default changes.
      storage.removeItem(AXIS_KEY);
    } else {
      storage.setItem(AXIS_KEY, axis);
    }
  } catch {
    // Session-only preference; nothing to do.
  }
}

/** The columns shut on this device, keyed by the column's own label. Anything
 * unparseable — or parseable but not an array of strings — reads as "none
 * collapsed", the default. */
export function readCollapsedColumns(storage: StorageLike | undefined): ReadonlySet<string> {
  if (!storage) {
    return new Set();
  }
  try {
    const stored = storage.getItem(COLLAPSED_KEY);
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
  collapsed: ReadonlySet<string>,
): void {
  if (!storage) {
    return;
  }
  try {
    if (collapsed.size === 0) {
      // Nothing collapsed is the default — an absent key says the same thing.
      storage.removeItem(COLLAPSED_KEY);
    } else {
      storage.setItem(COLLAPSED_KEY, JSON.stringify([...collapsed]));
    }
  } catch {
    // Session-only preference; nothing to do.
  }
}
