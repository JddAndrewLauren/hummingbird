// When to stop trying to re-mint silently, and start waiting for the reader.
//
// **Why this is needed even though the redirect flow shipped.** The redirect
// does not fix the silent path: `prompt=none` over a redirect is a full-page
// navigation, and a full-page navigation cannot run hourly in the background.
// So the silent re-mint stays iframe-based, and under iOS ITP a third-party
// iframe to accounts.google.com has no access to the Google session cookie —
// it loads and never posts back. Without this the app throws the reader into
// the interactive flow roughly every 55 minutes for as long as the page stays
// open.
//
// **What a block does and does not outlive.** Nothing here is persisted: the
// counter is a `useRef` in `shell/useCalendarWiring.ts` and the published flag
// is a non-persisted store field, so a block lasts exactly one page load. Under
// the ITP case above, every cold start therefore burns
// `SILENT_REMINT_FAILURE_LIMIT` dead iframe waits — two of them, at `gis.ts`'s
// 15-second silent timeout — before it re-blocks. That is the deliberate
// trade, since persisting a block would need a rule for lifting it that no
// signal available here can supply; but it is a per-load win, not a permanent
// one, and anyone reading this to size the ITP problem should size it that way.
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
  // Google answered, and the answer had no token in it (`gis.ts` mints this
  // when the callback fires with neither `access_token` nor an `error`). It is
  // in the blocking set because `connect-error.ts` — the module that owns what
  // this state means — already tells the reader to remove the app's calendar
  // access and connect again, and an hourly silent retry of something whose
  // documented remedy is a human in a Google account settings page is exactly
  // the nag this module exists to stop.
  "no_access_token",
]);

/** Errors that say nothing about whether the reader is needed — the script
 * did not load, the global was missing, the request could not start. These
 * are network and environment failures, and a merely-offline device must not
 * be pushed into an interactive flow it cannot complete. */
const NON_BLOCKING_ERRORS: ReadonlySet<string> = new Set([
  "gis_script_load_failed",
  "gis_unavailable",
  "gis_request_failed",
]);

/** What one error code means for the silent path. Three-valued on purpose:
 * `unclassified` is a real answer, not a synonym for `non-blocking`, and the
 * distinction is what makes `NON_BLOCKING_ERRORS` load-bearing rather than
 * decorative. The reducer below treats both as "does not count", but only a
 * membership in `NON_BLOCKING_ERRORS` says somebody looked at the code and
 * decided; `unclassified` says nobody has, which is what the test asserts is
 * true of no code the silent path can actually produce.
 *
 * Not classified here, deliberately: `popup_closed` and `popup_failed_to_open`
 * (a `prompt: "none"` request opens no popup, so the silent path cannot mint
 * them) and `redirect-flow.ts`'s `state_mismatch`/`no_expiry` (that flow is a
 * full-page navigation, never silent, and its outcomes reach
 * `recordInteractiveConnect` instead). Those are interactive-path codes and
 * `connect-error.ts` is where they get their meaning. */
export type RemintErrorClass = "blocking" | "non-blocking" | "unclassified";

export function classifyRemintError(error: string): RemintErrorClass {
  if (BLOCKING_ERRORS.has(error)) {
    return "blocking";
  }
  if (NON_BLOCKING_ERRORS.has(error)) {
    return "non-blocking";
  }
  return "unclassified";
}

export interface RemintHealth {
  consecutiveFailures: number;
  blocked: boolean;
}

export const INITIAL_REMINT_HEALTH: RemintHealth = { consecutiveFailures: 0, blocked: false };

/** Folds one silent re-mint outcome into the running health. `error` is
 * `null` for a success.
 *
 * Any success resets, completely — a silent one here, an interactive one
 * through `recordInteractiveConnect` below. The reader signed in again
 * somewhere else, ITP's window reopened, the session came back, or they simply
 * pressed Reconnect and completed it. Whatever the reason, the evidence for
 * blocking is gone, and the claim is only true because BOTH entry points
 * exist: for a while this one was the only one, and a successful interactive
 * reconnect left the block standing for the rest of the page's life. */
export function recordSilentRemint(health: RemintHealth, error: string | null): RemintHealth {
  if (error === null) {
    return INITIAL_REMINT_HEALTH;
  }
  if (classifyRemintError(error) !== "blocking") {
    // Neither counts nor resets. An offline stretch must not accumulate
    // toward a block — but it is also no evidence that the session recovered,
    // so a run of failures interrupted by an offline one is still a run. An
    // `unclassified` code lands here too: default-deny, because a code nobody
    // has classified is not evidence a human is required, and guessing wrong
    // nags the reader every hour.
    return health;
  }
  const consecutiveFailures = health.consecutiveFailures + 1;
  return {
    consecutiveFailures,
    blocked: consecutiveFailures >= SILENT_REMINT_FAILURE_LIMIT,
  };
}

/** Folds an *interactive* connect attempt — the Connect/Reconnect button, by
 * either the popup or the redirect route — into the running health.
 *
 * Only a success is evidence, and it is the same evidence a silent success is:
 * a token was minted, so whatever was stopping the silent path (a lapsed Google
 * session, a consent that had been revoked) is no longer true, and the block
 * must come off or the calendar goes stale until the next page load with
 * nothing on screen saying why.
 *
 * A failure is deliberately NOT counted. A cancelled popup, a declined consent
 * or a closed window says what the reader chose to do; it says nothing about
 * whether a `prompt: "none"` iframe could have got a token. Feeding those into
 * the silent counter would let two cancelled Reconnects block a path that was
 * never tried. */
export function recordInteractiveConnect(health: RemintHealth, error: string | null): RemintHealth {
  return error === null ? INITIAL_REMINT_HEALTH : health;
}

/** Every error the blocking set recognises, for tests and for anyone adding a
 * member — the two sets must stay disjoint. */
export function blockingRemintErrors(): readonly string[] {
  return [...BLOCKING_ERRORS];
}

/** Every error deliberately classified as network/environment. Exported for
 * the same reason as `blockingRemintErrors`: the test drives the reducer off
 * these lists, so a code added to one set without a decision about the other
 * fails there rather than at 3am on someone's phone. */
export function nonBlockingRemintErrors(): readonly string[] {
  return [...NON_BLOCKING_ERRORS];
}
