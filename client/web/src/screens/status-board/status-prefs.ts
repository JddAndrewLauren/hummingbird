import type { StorageLike } from "../storage";

// Which Status tile is open on this device — the board's one view
// preference, in the injectable-`storage` idiom `screens/frontier-prefs.ts`
// documents: device-local, never the `settings` table (no DELETE, syncs
// everywhere), and tolerant of a storage that throws or is absent, in which
// case the preference still applies for the session and resets on reload.
//
// One key holding one pane key, because the board's expansion is
// single-selection: opening a tile closes whichever was open. That is a
// different state shape from `screens/questions/collapse.ts`'s per-band
// override map, which is why this is a new key rather than a reuse — a
// band-stamped override answers "has the reader overruled the default for
// this pane, while it stays in this band", and the board has no per-pane
// default to overrule.
//
// **Shipped `hb.questions.collapse` entries suffixed `.status` are left
// where they are.** They are view preferences: absence is the default, so a
// dead key costs a reader nothing, and a migration would be a write against
// every device to delete something no code reads.

const EXPANDED_KEY = "hb.status.expanded";

/** The tile open on this device, or `null` for none — the default, encoded
 * as key absence so it cannot rot into a stale stored value. A stored pane
 * key that no longer ranks simply matches no tile, which reads as nothing
 * open; the board does not prune it, on the same reasoning
 * `PaneCollapse` keeps a band-mismatched override: the pane may come back. */
export function readExpandedKey(
  storage: StorageLike | undefined,
): string | null {
  if (!storage) {
    return null;
  }
  try {
    return storage.getItem(EXPANDED_KEY);
  } catch {
    return null;
  }
}

export function writeExpandedKey(
  storage: StorageLike | undefined,
  paneKey: string | null,
): void {
  if (!storage) {
    return;
  }
  try {
    if (paneKey === null) {
      storage.removeItem(EXPANDED_KEY);
    } else {
      storage.setItem(EXPANDED_KEY, paneKey);
    }
  } catch {
    // Session-only preference; nothing to do.
  }
}
