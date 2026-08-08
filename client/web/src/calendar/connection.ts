import { isTokenResult, type TokenClient } from "../google/gis";

// Issue #73's consent/token-rotation orchestration, kept free of GIS and
// the wasm worker so it is unit-testable against a fake `TokenClient` and a
// spyable `pushToken` — the same discipline as calendar-worker.ts.
//
// GIS issues browser SPAs no refresh token (Agent Brief's "Key interfaces"
// note): silent re-mint (`prompt: "none"`) is the only way to get a fresh
// access token without interrupting the user, and it only works while the
// user's Google session is still live. When it fails, the host's job is to
// surface a re-connect affordance rather than silently going dark.

export interface ConnectionResult {
  connected: boolean;
  needsReconnect: boolean;
  /** When the pushed token expires, or `null` if nothing was pushed (a
   * failed/declined attempt). The caller (App.tsx) uses this with
   * `msUntilRotation` to schedule the next proactive silent re-mint. */
  expiresAtMs: number | null;
}

export interface ConnectionDeps {
  tokenClient: TokenClient;
  pushToken: (token: string) => void;
}

const STILL_NEEDS_RECONNECT: ConnectionResult = {
  connected: true,
  needsReconnect: true,
  expiresAtMs: null,
};
const NOT_CONNECTED: ConnectionResult = {
  connected: false,
  needsReconnect: false,
  expiresAtMs: null,
};

/** Core-start wiring: if this device was previously opted in (a persisted
 * flag the host owns, not a credential — see `calendar/persistence.ts`),
 * attempt a silent re-mint and push the token at init. A never-opted-in
 * device does nothing here and stays disconnected. */
export async function initConnection(
  deps: ConnectionDeps,
  wasPreviouslyConnected: boolean,
): Promise<ConnectionResult> {
  if (!wasPreviouslyConnected) {
    return NOT_CONNECTED;
  }
  return silentReconnect(deps);
}

/** The interactive "Connect"/"Reconnect" affordance: an explicit consent
 * request. Used both for first-time opt-in and for recovering from a
 * failed silent re-mint. */
export async function connect(deps: ConnectionDeps): Promise<ConnectionResult> {
  const result = await deps.tokenClient.requestToken("consent");
  if (!isTokenResult(result)) {
    return NOT_CONNECTED;
  }
  deps.pushToken(result.accessToken);
  return { connected: true, needsReconnect: false, expiresAtMs: result.expiresAtMs };
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
    return STILL_NEEDS_RECONNECT;
  }
  deps.pushToken(result.accessToken);
  return { connected: true, needsReconnect: false, expiresAtMs: result.expiresAtMs };
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
