// Now's standing-questions aside: is the whole section shut?
//
// The same device-local view preference idiom as `screens/frontier-prefs.ts`
// (and `shell/rail-collapse.ts` before it) and for the same reason — a panel
// you shut on a narrow laptop says nothing about the phone in your pocket, and
// the `settings` table has no DELETE, so a per-surface toggle would accrete
// there forever.
//
// Open is the default and is encoded as key *absence*: the aside holds the
// standing questions, and a question that has fired is the one thing on Now
// that you did not ask for and must not have hidden from you by a stale
// preference from a previous build.

import type { StorageLike } from "../storage";

const COLLAPSED_KEY = "hb.questions.aside-collapsed";

/** True only for the exact stored marker — anything else, including garbage,
 * reads as open. */
export function readAsideCollapsed(storage: StorageLike | undefined): boolean {
  if (!storage) {
    return false;
  }
  try {
    return storage.getItem(COLLAPSED_KEY) === "1";
  } catch {
    return false;
  }
}

export function writeAsideCollapsed(
  storage: StorageLike | undefined,
  collapsed: boolean,
): void {
  if (!storage) {
    return;
  }
  try {
    if (collapsed) {
      storage.setItem(COLLAPSED_KEY, "1");
    } else {
      storage.removeItem(COLLAPSED_KEY);
    }
  } catch {
    // Session-only preference; nothing to do.
  }
}
