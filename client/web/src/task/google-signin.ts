import type { StorageLike } from "../screens/storage";

// The web leg of signing in with Google (ADR-0040 decision 3, #840): a
// full-page redirect to Google's authorization endpoint asking for an ID
// token, and the `/signin` return that reads it back off the URL fragment.
// Not Google Identity Services' button — that needs `accounts.google.com`
// in `script-src`, `connect-src` and `frame-src`, which #577/#586 removed
// from the CSP on purpose. A top-level navigation is governed by none of
// them, so `csp-worker/csp.ts` does not change.
//
// Pure and storage-injected, the `token.ts` discipline: the caller hands in
// `sessionStorage` and the fragment, and `main.tsx` is the one place that
// reads either off the real page. The client never verifies the token's
// signature — the authority does, and decides whose it is. What the client
// owns is the nonce: it is minted here, kept in `sessionStorage` (one tab,
// gone with it), read back out of the token's own claims, and consumed by
// the first read, so the same return URL reached twice (Back, a reload) is
// refused rather than exchanged again.

declare const __GOOGLE_SIGNIN_CLIENT_ID__: string;

/** The `hummingbird-signin` Web client id (#836) — `vite.config.ts`'s
 * build-time `define`, the same id the authority's
 * `GOOGLE_SIGNIN_CLIENT_IDS` accepts. `null` where the build supplies none
 * (vitest), which draws the paste form alone. */
export const GOOGLE_SIGNIN_CLIENT_ID: string | null =
  typeof __GOOGLE_SIGNIN_CLIENT_ID__ === "string" ? __GOOGLE_SIGNIN_CLIENT_ID__ : null;

/** The app route Google returns to. Registered as a redirect URI on the
 * `hummingbird-signin` client for each origin it serves (#836). */
export const SIGNIN_PATH = "/signin";

/** Where the pending sign-in's nonce waits for its return. */
export const NONCE_KEY = "hummingbird.signin-nonce";

const AUTHORIZATION_ENDPOINT = "https://accounts.google.com/o/oauth2/v2/auth";

/** What a `/signin` return carried: an ID token whose nonce checked out, or
 * the readable reason there is none. */
export type SignInReturn = { kind: "token"; idToken: string } | { kind: "error"; message: string };

/** Google's authorization URL for an ID token (`response_type=id_token`,
 * scope `openid email`) bound to `nonce`, returning to `redirectUri`. */
export function authorizationUrl(clientId: string, redirectUri: string, nonce: string): string {
  const params = new URLSearchParams({
    client_id: clientId,
    redirect_uri: redirectUri,
    response_type: "id_token",
    scope: "openid email",
    nonce,
  });
  return `${AUTHORIZATION_ENDPOINT}?${params.toString()}`;
}

/** Starts a sign-in: keeps a fresh nonce in `storage` and answers the URL
 * to navigate the page to. `origin` is the page's own, so the return lands
 * on the same origin the token store and the queue belong to. */
export function beginGoogleSignIn(storage: StorageLike, clientId: string, origin: string): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  const nonce = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
  storage.setItem(NONCE_KEY, nonce);
  return authorizationUrl(clientId, `${origin}${SIGNIN_PATH}`, nonce);
}

/** Reads a `/signin` return against the nonce kept for it, and consumes
 * that nonce whatever the outcome — one return, one read. */
export function readSignInReturn(hash: string, storage: StorageLike): SignInReturn {
  const expected = storage.getItem(NONCE_KEY);
  storage.removeItem(NONCE_KEY);
  return parseSignInFragment(hash, expected);
}

/** The decision half of [`readSignInReturn`]: `hash` is the URL fragment
 * (with or without its `#`), `expectedNonce` the one kept for this attempt
 * or `null` when none is waiting. */
export function parseSignInFragment(hash: string, expectedNonce: string | null): SignInReturn {
  const params = new URLSearchParams(hash.startsWith("#") ? hash.slice(1) : hash);
  const error = params.get("error");
  if (error !== null) {
    if (error === "access_denied") {
      return { kind: "error", message: "Sign-in was cancelled." };
    }
    return {
      kind: "error",
      message: `Google refused the sign-in: ${params.get("error_description") ?? error}`,
    };
  }
  const idToken = params.get("id_token");
  if (idToken === null || idToken === "") {
    return { kind: "error", message: "Google sent no sign-in token back. Sign in again." };
  }
  if (expectedNonce === null) {
    return {
      kind: "error",
      message: "This sign-in link was already used or has expired. Sign in again.",
    };
  }
  const nonce = nonceClaim(idToken);
  if (nonce === null || nonce !== expectedNonce) {
    return {
      kind: "error",
      message: "This sign-in did not start on this page, so it was not used. Sign in again.",
    };
  }
  return { kind: "token", idToken };
}

/** The `nonce` claim of an unverified JWT, or `null` when the token cannot
 * be read or carries none. */
function nonceClaim(jwt: string): string | null {
  const parts = jwt.split(".");
  if (parts.length !== 3) {
    return null;
  }
  try {
    const base64 = parts[1].replace(/-/g, "+").replace(/_/g, "/");
    const binary = atob(base64.padEnd(base64.length + ((4 - (base64.length % 4)) % 4), "="));
    const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
    const claims: unknown = JSON.parse(new TextDecoder().decode(bytes));
    if (typeof claims === "object" && claims !== null && "nonce" in claims) {
      return typeof claims.nonce === "string" ? claims.nonce : null;
    }
    return null;
  } catch {
    return null;
  }
}

/** The name this browser signs in as — `Chrome on macOS` — derived from its
 * user agent, the order mattering because every Chromium browser also says
 * `Chrome` and every browser on Apple's platforms also says `Safari`. The
 * authority slugs it into the token id (`device-web-chrome-on-macos`). */
export function deviceNameFromUserAgent(userAgent: string): string {
  const browser = /Edg\//.test(userAgent)
    ? "Edge"
    : /Firefox\/|FxiOS\//.test(userAgent)
      ? "Firefox"
      : /Chrome\/|CriOS\//.test(userAgent)
        ? "Chrome"
        : /Safari\//.test(userAgent)
          ? "Safari"
          : null;
  const os = /Android/.test(userAgent)
    ? "Android"
    : /iPhone|iPad|iPod/.test(userAgent)
      ? "iOS"
      : /CrOS/.test(userAgent)
        ? "ChromeOS"
        : /Windows/.test(userAgent)
          ? "Windows"
          : /Mac OS X|Macintosh/.test(userAgent)
            ? "macOS"
            : /Linux/.test(userAgent)
              ? "Linux"
              : null;
  if (browser === null) {
    return "Web browser";
  }
  return os === null ? browser : `${browser} on ${os}`;
}

/** The sign-in outcomes that are not `signed_in`: the core's arms
 * (`SignInOutcome::kind`) plus the seam's `busy` and the task queue's
 * `abandoned` (#889). */
export type SignInFailureKind =
  | "not_allowed"
  | "rate_limited"
  | "unconfigured"
  | "unavailable"
  | "invalid"
  | "failed"
  | "busy"
  | "abandoned";

/** What the device-token card says when the exchange did not mint a token.
 * `detail` is the core's own message on `invalid`/`failed`. */
export function signInFailureMessage(kind: SignInFailureKind, detail: string | null): string {
  switch (kind) {
    case "not_allowed":
      return "This Google account is not allowed to sign in to hummingbird.";
    case "rate_limited":
      return "Too many sign-ins in the last hour. Wait a while, or paste a token instead.";
    case "unconfigured":
      return "Sign-in with Google is not set up on the server. Paste a token instead.";
    case "unavailable":
      return "The server could not check the sign-in right now. Try again.";
    case "invalid":
      return `The server refused the sign-in request: ${detail ?? "invalid request"}`;
    case "failed":
      return `The sign-in did not reach the server: ${detail ?? "unknown error"}`;
    case "busy":
      return "The local core was busy. Try again.";
    case "abandoned":
      return "Sign-in didn't finish. Try again, or paste a token instead.";
  }
}
