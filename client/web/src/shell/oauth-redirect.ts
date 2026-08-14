import {
  buildAuthorizeUrl,
  createState,
  OAUTH_STATE_KEY,
  parseRedirectFragment,
  type RedirectOutcome,
} from "../google/redirect-flow";

// The browser glue around `google/redirect-flow.ts`'s pure half: read the
// fragment, park and check the nonce, and navigate. Everything decidable is
// over there and unit-tested; this file is `location`, `sessionStorage` and
// `crypto`, and nothing else.
//
// **Consumed before React mounts.** `main.tsx` calls `captureOAuthRedirect()`
// at module scope, for two reasons. The fragment must be off the URL before
// anything can log it, share it or restore it from history — it carries an
// access token. And the return from Google is a full page load, so there is no
// component alive to receive it: the outcome is parked here and the calendar
// wiring picks it up when the core reports ready.
//
// That full page load also means the SharedWorker and the wasm core reboot and
// any in-memory screen state is lost. Acceptable for a gesture that happens at
// most once an hour, and only on the surface where the popup flow cannot work
// at all.

let captured: RedirectOutcome = { kind: "none" };

/** Reads and CLEARS the redirect fragment. Idempotent — a second call returns
 * `none`, since the first one already took it off the URL. */
export function captureOAuthRedirect(): void {
  const outcome = parseRedirectFragment(
    window.location.hash,
    sessionStorage.getItem(OAUTH_STATE_KEY),
    Date.now(),
  );
  if (outcome.kind === "none") {
    return;
  }
  captured = outcome;
  // One-shot: a parked nonce that outlived its attempt would validate a
  // fragment from a later, unrelated navigation.
  sessionStorage.removeItem(OAUTH_STATE_KEY);
  // `replaceState`, not `pushState`: the fragment must not be somewhere the
  // back button can return to. `location.pathname + location.search` rather
  // than `"#"`, which would leave a bare hash on the URL.
  window.history.replaceState(null, "", window.location.pathname + window.location.search);
}

/** Hands over whatever `captureOAuthRedirect` found, exactly once. The caller
 * is `useCalendarWiring`'s start effect, which runs under React's StrictMode
 * double-invoke in development — so this must not be re-readable, or the same
 * token would be applied twice and the second application would race the first
 * one's poll. */
export function takeOAuthRedirect(): RedirectOutcome {
  const outcome = captured;
  captured = { kind: "none" };
  return outcome;
}

/** Parks a fresh nonce and navigates to Google. Does not return in any useful
 * sense — the document is being replaced. */
export function startOAuthRedirect(clientId: string): void {
  const state = createState((array) => crypto.getRandomValues(array));
  sessionStorage.setItem(OAUTH_STATE_KEY, state);
  window.location.assign(
    buildAuthorizeUrl({
      clientId,
      // The origin root, with its trailing slash — byte-matching what is
      // registered on the OAuth client. `location.origin` never carries one.
      redirectUri: `${window.location.origin}/`,
      state,
    }),
  );
}
