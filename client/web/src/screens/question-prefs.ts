// Which standing-question rows are open in Settings' roster (#715,
// ADR-0034) — a view preference on one device, in the injectable-`storage`
// idiom (`screens/storage.ts`'s `StorageLike`) that `frontier-prefs.ts` and
// `questions/collapse.ts` already use.
//
// **Never a binding, and never a `settings` row.** `bindings.rs` is explicit
// that the collapse state of a pane is device-local and band-scoped, and
// this slice is where the temptation is sharpest: the row it belongs to is
// the very row whose toggle *does* sync. They are different facts. One says
// whether a question is asked at all, on every device; the other says
// whether one reader has this row open on this screen on this laptop right
// now. Putting the second in `settings` would accrete a permanent key per
// question in a table with no DELETE, and follow the reader onto a phone
// whose roster is a different shape.
//
// **Open is stored, shut is absent.** Rows are collapsed by default, so the
// default is key absence — the same encoding `frontier-prefs.ts` uses for
// its own defaults, and for the same reason: a default encoded as a stored
// value rots the day the default changes.
//
// Every call tolerates a broken or absent storage (private-mode Safari
// throws on `setItem`): a preference that cannot persist still applies for
// the session, it just resets on reload.

import type { StorageLike } from "./storage";

const EXPANDED_KEY = "hb.settings.questions-expanded";

/** The question rows open on this device, keyed by the question's own wire
 * spelling. Anything unparseable — or parseable but not an array of strings
 * — reads as "none open", the default. */
export function readExpandedQuestions(storage: StorageLike | undefined): ReadonlySet<string> {
  if (!storage) {
    return new Set();
  }
  try {
    const stored = storage.getItem(EXPANDED_KEY);
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

export function writeExpandedQuestions(
  storage: StorageLike | undefined,
  expanded: ReadonlySet<string>,
): void {
  if (!storage) {
    return;
  }
  try {
    if (expanded.size === 0) {
      // Nothing open is the default — an absent key says the same thing.
      storage.removeItem(EXPANDED_KEY);
    } else {
      storage.setItem(EXPANDED_KEY, JSON.stringify([...expanded]));
    }
  } catch {
    // Session-only preference; nothing to do.
  }
}

/** One row's open state flipped — returned as a new set, so the caller's
 * state update is a plain replacement. Not pruned against the live roster
 * the way `frontier-prefs.ts` prunes its columns: the question vocabulary is
 * closed and bounded at ten, so a stale entry costs a few bytes and comes
 * back into use if the question ever returns. */
export function toggleExpandedQuestion(
  expanded: ReadonlySet<string>,
  question: string,
): ReadonlySet<string> {
  const next = new Set(expanded);
  if (!next.delete(question)) {
    next.add(question);
  }
  return next;
}
