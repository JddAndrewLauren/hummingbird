// When to stop trying to re-mint silently, and start waiting for the reader.
//
// **Rewritten for #577/#583.** The mechanism this module implements is
// still earned even though `google/gis.ts` and its ITP problem are gone: a
// revoked `GOOGLE_CALENDAR_REFRESH_TOKEN` (ADR-0028) would make
// `POST /api/google/calendar_token` answer `authority_upstream` every
// single hour, forever, with no human ever told. What changed is only the
// error vocabulary — GIS's popup/iframe codes retire, and
// `calendar/authority-token-client.ts`'s seven codes take their place.
//
// **What a block does and does not outlive.** Nothing here is persisted: the
// counter is a `useRef` in `shell/useCalendarWiring.ts` and the published
// flag is a non-persisted store field, so a block lasts exactly one page
// load. `needsReconnect` is deliberately left standing, so the Reconnect
// affordance and the last-good snapshot both survive — the app keeps
// showing stale-but-honest context rather than going dark.
//
// The rule: two consecutive failures that mean "a human has to be involved"
// blocks the silent path.
//
// **What counts is the whole design.** Only errors that say *a human is
// required* count. An error that says *the network/authority is unavailable*
// must not, because blocking on those would nag a merely-offline reader into
// an interactive Connect they cannot complete anyway.

/** Two, not one: a single failure is routinely transient (a request that
 * raced a Durable Object cold start, a blip talking to the authority), and
 * blocking on it would turn one hiccup into a manual reconnect. Two
 * consecutive is a state, not an event. */
export const SILENT_REMINT_FAILURE_LIMIT = 2;

/** Errors that mean the silent path cannot succeed without the reader. */
const BLOCKING_ERRORS: ReadonlySet<string> = new Set([
  // No device token stored on this device — nothing a silent retry can fix;
  // the reader has to enter one in Settings.
  "no_device_token",
  // The authority rejected the device token itself (401/403). ADR-0004: a
  // device token that is bad stays bad until the reader re-enters one.
  "authority_rejected_device_token",
  // `GOOGLE_CALENDAR_CLIENT_ID`/`_SECRET`/`_REFRESH_TOKEN` unset on the
  // authority (503) — an operator problem, not a transient one.
  "authority_unconfigured",
  // The authority reached Google and Google said no — unreachable,
  // `invalid_grant`, or any other non-2xx (502, `google_calendar.rs`'s three
  // cases collapsed to one code here). A dead or revoked refresh token is
  // exactly the "502 every hour forever" case this module's header names,
  // and retrying it hourly is the nag this limit exists to stop.
  "authority_upstream",
  // A 200 whose body parsed but carried no usable token — structurally
  // wrong, not a blip. `connect-error.ts` already tells the reader what to
  // do about it.
  "no_access_token",
]);

/** Errors that say nothing about whether the reader is needed — the request
 * could not reach the authority at all, or its answer could not be read.
 * These are transport/environment failures, and a merely-offline device
 * must not be pushed into an interactive flow it cannot complete either. */
const NON_BLOCKING_ERRORS: ReadonlySet<string> = new Set([
  // `fetch` rejected or timed out — offline, DNS, the request timeout in
  // `authority-token-client.ts`.
  "authority_unreachable",
  // A non-2xx this client did not recognise, or a 200 body that was not
  // readable JSON at all — worth a retry, since the authority itself may
  // simply have hiccuped.
  "bad_token_response",
]);

/** What one error code means for the silent path. Three-valued on purpose:
 * `unclassified` is a real answer, not a synonym for `non-blocking`, and the
 * distinction is what makes `NON_BLOCKING_ERRORS` load-bearing rather than
 * decorative. The reducer below treats both as "does not count", but only a
 * membership in `NON_BLOCKING_ERRORS` says somebody looked at the code and
 * decided; `unclassified` says nobody has, which is what the test asserts is
 * true of no code the silent path can actually produce. */
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
 * through `recordInteractiveConnect` below. Whatever the reason, the
 * evidence for blocking is gone, and the claim is only true because BOTH
 * entry points exist: for a while this one was the only one, and a
 * successful interactive reconnect left the block standing for the rest of
 * the page's life. */
export function recordSilentRemint(health: RemintHealth, error: string | null): RemintHealth {
  if (error === null) {
    return INITIAL_REMINT_HEALTH;
  }
  if (classifyRemintError(error) !== "blocking") {
    // Neither counts nor resets. An offline stretch must not accumulate
    // toward a block — but it is also no evidence that the session
    // recovered, so a run of failures interrupted by an offline one is still
    // a run. An `unclassified` code lands here too: default-deny, because a
    // code nobody has classified is not evidence a human is required, and
    // guessing wrong nags the reader every hour.
    return health;
  }
  const consecutiveFailures = health.consecutiveFailures + 1;
  return {
    consecutiveFailures,
    blocked: consecutiveFailures >= SILENT_REMINT_FAILURE_LIMIT,
  };
}

/** Folds an *interactive* connect attempt — the Connect/Reconnect button —
 * into the running health.
 *
 * Only a success is evidence, and it is the same evidence a silent success
 * is: a token was minted, so whatever was stopping the silent path is no
 * longer true, and the block must come off or the calendar goes stale until
 * the next page load with nothing on screen saying why.
 *
 * A failure is deliberately NOT counted. It says what happened on this one
 * attempt; it says nothing about whether the *silent* path could have got a
 * token. Feeding those into the silent counter would let two failed
 * Reconnects block a path that was never tried. */
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
