// Whether the nav rail is collapsed to its icons-and-counts form — a view
// preference on one device, not a cross-device fact, so it lives in the
// injectable-`storage` idiom `screens/questions/collapse.ts` and
// `theme/theme.ts` already use, never in the `settings` table (which has no
// DELETE and syncs everywhere).
//
// Every call tolerates a broken or absent storage (private-mode Safari
// throws on `setItem`; a worker context has none): a preference that cannot
// persist still applies for the session, it just resets on reload.

const RAIL_COLLAPSE_KEY = "hb.shell.rail-collapsed";

/** The narrow slice of `localStorage` this module needs — redeclared rather
 * than imported (`collapse.ts`'s own reasoning) so the shell chrome depends
 * on nothing question- or calendar-shaped. */
export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

export function readRailCollapsed(storage: StorageLike | undefined): boolean {
  if (!storage) {
    return false;
  }
  try {
    return storage.getItem(RAIL_COLLAPSE_KEY) === "1";
  } catch {
    return false;
  }
}

export function writeRailCollapsed(storage: StorageLike | undefined, collapsed: boolean): void {
  if (!storage) {
    return;
  }
  try {
    if (collapsed) {
      storage.setItem(RAIL_COLLAPSE_KEY, "1");
    } else {
      // Expanded is the default — an absent key says the same thing, and
      // never rots into a stale legacy value.
      storage.removeItem(RAIL_COLLAPSE_KEY);
    }
  } catch {
    // Session-only preference; nothing to do.
  }
}
