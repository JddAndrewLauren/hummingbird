// The redirect half of the calendar connection: build an authorize URL, read
// what comes back in the fragment, and check it is ours.
//
// **Why a redirect at all.** GIS's `initTokenClient` is popup-based with no
// `ux_mode` option. In an installed iOS web app the popup escapes to Safari
// and loses its opener, so the flow can never complete — and connecting in a
// Safari tab does not help either, because an installed iOS web app has a
// storage container separate from Safari's. A top-level navigation is the only
// mechanism that starts and finishes inside the installed window. GIS stays
// the desktop path, where it works and is less disruptive.
//
// **`response_type=token`, not auth-code + PKCE.** Google requires a client
// secret at the token endpoint for Web-application clients, and a client
// secret cannot live in a browser. So this is the implicit flow: the same
// 1-hour access token GIS already yields, no refresh token, no new credential
// class, and the blast radius is unchanged — one per-device `calendar.readonly`
// token. Nothing new goes into Actions secrets.
//
// **No router and no `/oauth/callback` route.** The redirect URI is the origin
// root, and `main.tsx` reads `location.hash` before React mounts. That
// sidesteps `not_found_handling` entirely, needs no CSP change (`form-action
// 'none'` does not constrain `location.assign`), and keeps this module pure:
// everything here is a string in and a value out, and the browser glue is the
// caller's.
//
// The operator step this depends on: the existing Internal, Web-application
// OAuth client (`VITE_GOOGLE_CLIENT_ID`, NOT the sweeper's desktop client)
// must list `https://hb.twinion.net/` and `http://localhost:5173/` as
// Authorized redirect URIs — exact match, no fragment. Done 2026-08-13.

import { CALENDAR_READONLY_SCOPE } from "./gis";

const AUTHORIZE_ENDPOINT = "https://accounts.google.com/o/oauth2/v2/auth";

/** Where the one-shot `state` value is parked across the navigation.
 * `sessionStorage`, chosen by the caller: it is scoped to this tab/window and
 * cleared when it closes, which is exactly the lifetime of one attempt. */
export const OAUTH_STATE_KEY = "hb.google.oauth-state";

export interface AuthorizeParams {
  clientId: string;
  /** Must byte-match a URI registered on the OAuth client. The origin root,
   * with its trailing slash. */
  redirectUri: string;
  /** The CSRF nonce, echoed back by Google and checked on return. */
  state: string;
}

export function buildAuthorizeUrl({ clientId, redirectUri, state }: AuthorizeParams): string {
  const url = new URL(AUTHORIZE_ENDPOINT);
  url.searchParams.set("client_id", clientId);
  url.searchParams.set("redirect_uri", redirectUri);
  url.searchParams.set("response_type", "token");
  url.searchParams.set("scope", CALENDAR_READONLY_SCOPE);
  url.searchParams.set("state", state);
  // `include_granted_scopes` keeps any scope already granted to this client
  // attached to the returned token, so re-connecting never silently narrows
  // what a previously-broader grant covered.
  url.searchParams.set("include_granted_scopes", "true");
  return url.toString();
}

export type RedirectOutcome =
  /** This load is not a return from Google. Much the commonest case — every
   * ordinary app open takes it. */
  | { kind: "none" }
  | { kind: "token"; accessToken: string; expiresAtMs: number }
  | { kind: "error"; error: string };

/** Reads a returned fragment. `expectedState` is what the caller parked before
 * navigating (`null` if nothing was parked). `nowMs` is injected rather than
 * sampled, the `gis.ts` rule — `expires_in` is a duration and only the caller
 * knows when the answer arrived.
 *
 * Anything unrecognised is `none`, not an error: this runs on every single app
 * open, including deep links and a stray `#section` anchor, and reporting a
 * connection failure to someone who never asked to connect would be a lie. */
export function parseRedirectFragment(
  hash: string,
  expectedState: string | null,
  nowMs: number,
): RedirectOutcome {
  const params = new URLSearchParams(hash.startsWith("#") ? hash.slice(1) : hash);
  const error = params.get("error");
  const accessToken = params.get("access_token");
  if (error === null && accessToken === null) {
    return { kind: "none" };
  }

  // State is checked before anything is believed, on the error path too: an
  // attacker-supplied fragment is as able to carry `error=access_denied` as a
  // token. A missing `expectedState` means this window never started a flow.
  //
  // Reporting such a fragment would let a third party put words in the app's
  // mouth, but that was never the sharp end of it, and reading this as though
  // it were leaves the real exposure looking closed. The sharp end is what the
  // CALLER does with an outcome of `kind: "error"`. Opening
  // `https://hb.twinion.net/#error=x&state=y` needs no interaction beyond the
  // click on the link, lands here, and comes out as a failed connection
  // attempt; the caller then decides what that means for the device's stored
  // state. Until `calendar/redirect-return.ts` existed, what it meant was
  // `writeConnected(localStorage, false)` — a link that dropped the reader's
  // calendar opt-in, their last-good snapshot and the Reconnect affordance.
  // `state_mismatch` below is this app's own CSRF check failing, and every
  // consumer of it should treat it as "believe nothing here", including
  // believing nothing about whether this device is still connected.
  const state = params.get("state");
  if (expectedState === null || state === null || state !== expectedState) {
    return { kind: "error", error: "state_mismatch" };
  }

  if (error !== null) {
    return { kind: "error", error };
  }
  // `access_token` is non-null here: the early return above covers the case
  // where both are absent, and `error` has just been handled.
  const expiresInSeconds = Number(params.get("expires_in"));
  if (!Number.isFinite(expiresInSeconds) || expiresInSeconds <= 0) {
    // A token whose lifetime is unknown is worse than none: the rotation timer
    // would be scheduled off a garbage expiry and either spin immediately or
    // never fire, and the poll would 401 with nothing watching.
    return { kind: "error", error: "no_expiry" };
  }
  return {
    kind: "token",
    accessToken: accessToken as string,
    expiresAtMs: nowMs + expiresInSeconds * 1000,
  };
}

/** A fresh `state` nonce. `crypto.getRandomValues` rather than `Math.random`:
 * this is the only thing standing between the app and a fragment somebody else
 * chose, and a predictable nonce is not a nonce. */
export function createState(randomValues: (array: Uint8Array) => Uint8Array): string {
  const bytes = randomValues(new Uint8Array(16));
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}
