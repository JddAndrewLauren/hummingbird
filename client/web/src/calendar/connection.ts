import { isTokenResult, type TokenClient } from "./token-client";

// Issue #73's consent/token-rotation orchestration, kept free of any
// particular token source and the wasm worker so it is unit-testable
// against a fake `TokenClient` (`./token-client.ts`) and a spyable
// `pushToken` — the same discipline as calendar-worker.ts.
//
// This code did not change when #577/#583 moved the token source from
// GIS to the authority (`calendar/authority-token-client.ts`) — that is
// the point of the `TokenClient` seam. Silent re-mint (`prompt: "none"`)
// is still how a fresh access token is asked for without interrupting the
// user; the current `TokenClient` ignores the distinction, but the calls
// here stay in case a future one cares again. When it fails, the host's
// job is to surface a re-connect affordance rather than silently going
// dark.

export interface ConnectionResult {
  connected: boolean;
  needsReconnect: boolean;
  /** When the pushed token expires, or `null` if nothing was pushed (a
   * failed/declined attempt). The caller (App.tsx) uses this with
   * `msUntilRotation` to schedule the next proactive silent re-mint. */
  expiresAtMs: number | null;
  /** The raw `TokenError.error` this attempt failed with, or `null` if it
   * succeeded. Raw and not pre-formatted: `calendar/connect-error.ts` owns
   * the words, and `remint-health.ts` matches on the code — both need the
   * code itself, and a sentence is not a code. */
  error: string | null;
}

export interface ConnectionDeps {
  tokenClient: TokenClient;
  pushToken: (token: string) => void;
}

function stillNeedsReconnect(error: string): ConnectionResult {
  return { connected: true, needsReconnect: true, expiresAtMs: null, error };
}
function notConnected(error: string | null): ConnectionResult {
  return { connected: false, needsReconnect: false, expiresAtMs: null, error };
}
function connected(expiresAtMs: number): ConnectionResult {
  return { connected: true, needsReconnect: false, expiresAtMs, error: null };
}

/** Core-start wiring: if this device was previously opted in (a persisted
 * flag the host owns, not a credential — see `calendar/persistence.ts`),
 * attempt a silent re-mint and push the token at init. A never-opted-in
 * device does nothing here and stays disconnected. */
export async function initConnection(
  deps: ConnectionDeps,
  wasPreviouslyConnected: boolean,
): Promise<ConnectionResult> {
  if (!wasPreviouslyConnected) {
    // Not a failure: this device was never opted in, and there is nothing to
    // report. `error: null` keeps "never tried" distinct from "tried and
    // failed", which is what the Settings message is gated on.
    return notConnected(null);
  }
  return silentReconnect(deps);
}

/** The interactive "Connect"/"Reconnect" affordance: an explicit consent
 * request. Used both for first-time opt-in and for recovering from a
 * failed silent re-mint. */
export async function connect(deps: ConnectionDeps): Promise<ConnectionResult> {
  const result = await deps.tokenClient.requestToken("consent");
  if (!isTokenResult(result)) {
    return notConnected(result.error);
  }
  deps.pushToken(result.accessToken);
  return connected(result.expiresAtMs);
}

/** Whether an interactive attempt's result should be discarded, leaving the
 * device's existing connection state alone.
 *
 * TWO callers, and they are easy to forget the second of. [`connect`]'s popup
 * result is one; the other is a return from the redirect flow, whose answer
 * arrives on the next page load and is resolved through
 * `calendar/redirect-return.ts` — that module's header explains why the two
 * share this question but not the remedy for it. For a while only the popup
 * consulted this, and a failed redirect return wiped exactly what the rest of
 * this doc says must never be wiped.
 *
 * The same button is "Connect" and "Reconnect". For a first-time opt-in a
 * declined/failed consent correctly ends disconnected. For a *reconnect* it
 * must not: writing `connected: false` there un-opts-in the device, drops
 * the persisted flag, and takes the last-good (stale but real) tile and the
 * Reconnect affordance itself down with it — so cancelling the Google popup
 * once would cost the user their offline context. The existing connection
 * stands until a reconnect actually succeeds.
 *
 * Note what this does NOT cover: the FAILURE itself. Keeping the connection
 * is about `connected`/`needsReconnect`, and the caller must still record
 * `result.error` — a reconnect that failed is precisely when the reader needs
 * telling, and this returning `true` used to mean the handler returned before
 * touching any state at all. `useCalendarWiring.ts`'s `handleConnectClick`
 * writes the error above this check for that reason; do not move it back
 * below. */
export function shouldKeepExistingConnection(
  wasConnected: boolean,
  result: ConnectionResult,
): boolean {
  return wasConnected && !result.connected;
}

/** Answers a credential-needed round-trip from the core (issue #72's
 * `CredentialEvent`, surfaced here as a `credentialEvents` worker message):
 * try a silent re-mint first, only falling back to flagging
 * `needsReconnect` — the UI's cue to offer the interactive `connect()`
 * affordance — if that fails. */
export async function handleCredentialNeeded(
  deps: ConnectionDeps,
): Promise<ConnectionResult> {
  return silentReconnect(deps);
}

async function silentReconnect(deps: ConnectionDeps): Promise<ConnectionResult> {
  const result = await deps.tokenClient.requestToken("none");
  if (!isTokenResult(result)) {
    return stillNeedsReconnect(result.error);
  }
  deps.pushToken(result.accessToken);
  return connected(result.expiresAtMs);
}

/** How long to wait, in milliseconds, before proactively re-minting a token
 * ahead of its expiry — GIS gives no refresh token, so this is what keeps a
 * long-lived session from ever hitting a live 401 in the first place.
 * `marginMs` is the safety margin before the real expiry (default 5
 * minutes); the result is clamped to 0 so an already-expired/near-expired
 * token schedules an immediate re-mint rather than a negative delay. */
export function msUntilRotation(
  expiresAtMs: number,
  nowMs: number,
  marginMs = 5 * 60 * 1000,
): number {
  return Math.max(0, expiresAtMs - marginMs - nowMs);
}
