// When to stop trying to re-mint silently, and start waiting for the reader.
//
// **Why this is needed even though the redirect flow shipped.** The redirect
// does not fix the silent path: `prompt=none` over a redirect is a full-page
// navigation, and a full-page navigation cannot run hourly in the background.
// So the silent re-mint stays iframe-based, and under iOS ITP a third-party
// iframe to accounts.google.com has no access to the Google session cookie —
// it loads and never posts back. Without this the app throws the reader into
// the interactive flow roughly every 55 minutes, forever.
//
// The rule: two consecutive failures that mean "a human has to be involved"
// blocks the silent path. `needsReconnect` is deliberately left standing, so
// the Reconnect affordance and the last-good snapshot both survive — the app
// keeps showing stale-but-honest context rather than going dark.
//
// **What counts is the whole design.** Only errors that say *a human is
// required* count. An error that says *the network is unavailable* must not,
// because blocking on those would nag a merely-offline reader into an
// interactive consent screen they cannot complete anyway — the same reasoning
// `gis.ts`'s de-memoisation comment already records for the script load.

import { TOKEN_TIMEOUT_ERROR } from "../google/gis";

/** Two, not one: a single failure is routinely transient (a session that is
 * being refreshed, a request that raced a sign-in elsewhere), and blocking on
 * it would turn one hiccup into a manual reconnect. Two consecutive is a
 * state, not an event. */
export const SILENT_REMINT_FAILURE_LIMIT = 2;

/** Errors that mean the silent path cannot succeed without the reader. */
const BLOCKING_ERRORS: ReadonlySet<string> = new Set([
  "interaction_required",
  "login_required",
  "consent_required",
  "access_denied",
  "user_logged_out",
  // A `prompt: "none"` timeout IS the ITP signature: the iframe loaded and
  // never posted back, which is exactly what a partitioned cookie jar looks
  // like from this side. Retrying it costs 15 seconds and can never succeed.
  TOKEN_TIMEOUT_ERROR,
]);

/** Errors that say nothing about whether the reader is needed — the script
 * did not load, the global was missing, the request could not start. These
 * are network and environment failures, and a merely-offline device must not
 * be pushed into an interactive flow it cannot complete. Named as a list
 * rather than left to the `else` branch so the distinction is a decision on
 * the page rather than an accident of set membership. */
const NON_BLOCKING_ERRORS: ReadonlySet<string> = new Set([
  "gis_script_load_failed",
  "gis_unavailable",
  "gis_request_failed",
]);

export interface RemintHealth {
  consecutiveFailures: number;
  blocked: boolean;
}

export const INITIAL_REMINT_HEALTH: RemintHealth = { consecutiveFailures: 0, blocked: false };

/** Folds one silent re-mint outcome into the running health. `error` is
 * `null` for a success.
 *
 * Any success resets, completely: the reader signed in again somewhere else,
 * ITP's window reopened, the session came back. Whatever the reason, the
 * evidence for blocking is gone. */
export function recordSilentRemint(health: RemintHealth, error: string | null): RemintHealth {
  if (error === null) {
    return INITIAL_REMINT_HEALTH;
  }
  if (!BLOCKING_ERRORS.has(error)) {
    // Neither counts nor resets. An offline stretch must not accumulate
    // toward a block — but it is also no evidence that the session recovered,
    // so a run of failures interrupted by an offline one is still a run.
    return health;
  }
  const consecutiveFailures = health.consecutiveFailures + 1;
  return {
    consecutiveFailures,
    blocked: consecutiveFailures >= SILENT_REMINT_FAILURE_LIMIT,
  };
}

/** Whether an error is one of the network/environment kind — exported so the
 * classification is testable directly, rather than only through the counter,
 * and so the two lists above can be asserted disjoint. */
export function isNonBlockingRemintError(error: string): boolean {
  return NON_BLOCKING_ERRORS.has(error);
}

/** Every error the blocking set recognises, for tests and for anyone adding a
 * member — the two sets must stay disjoint. */
export function blockingRemintErrors(): readonly string[] {
  return [...BLOCKING_ERRORS];
}
