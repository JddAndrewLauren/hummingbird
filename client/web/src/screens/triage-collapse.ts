// Whether Now's triage section is collapsed to its header — a view
// preference on one device, not a cross-device fact, so it lives in the
// injectable-`storage` idiom `shell/rail-collapse.ts`,
// `screens/questions/collapse.ts` and `theme/theme.ts` already use, never in
// the `settings` table (which has no DELETE and syncs everywhere).
//
// Deliberately NOT `screens/questions/collapse.ts`: that module's overrides
// are scoped to an ADR-0015 `PaneAnswer`'s computed band, and there is no
// band here — the triage section is one section with one state. Borrowing it
// would drag the standing-question shell into the frontier column for a
// single boolean.
//
// Every call tolerates a broken or absent storage (private-mode Safari throws
// on `setItem`; a worker context has none): a preference that cannot persist
// still applies for the session, it just resets on reload.

const TRIAGE_COLLAPSE_KEY = "hb.now.triage-collapsed";

/** The narrow slice of `localStorage` this module needs — redeclared rather
 * than imported (`rail-collapse.ts`'s own reasoning) so the frontier column
 * depends on nothing question- or shell-shaped. */
export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

export function readTriageCollapsed(storage: StorageLike | undefined): boolean {
  if (!storage) {
    return false;
  }
  try {
    return storage.getItem(TRIAGE_COLLAPSE_KEY) === "1";
  } catch {
    return false;
  }
}

export function writeTriageCollapsed(
  storage: StorageLike | undefined,
  collapsed: boolean,
): void {
  if (!storage) {
    return;
  }
  try {
    if (collapsed) {
      storage.setItem(TRIAGE_COLLAPSE_KEY, "1");
    } else {
      // Expanded is the default — an absent key says the same thing, and
      // never rots into a stale legacy value.
      storage.removeItem(TRIAGE_COLLAPSE_KEY);
    }
  } catch {
    // Session-only preference; nothing to do.
  }
}
