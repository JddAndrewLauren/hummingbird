// @vitest-environment jsdom

// The browser glue around `google/redirect-flow.ts`. The parsing is tested
// over there and is pure; what is only testable here is the part that touches
// the document — and that part carries three decisions a reader would not
// guess from the types: the fragment comes OFF the url, the parked nonce is
// one-shot, and so is the captured outcome.

import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { OAUTH_STATE_KEY } from "../google/redirect-flow";
import { captureOAuthRedirect, takeOAuthRedirect } from "./oauth-redirect";

const STATE = "abcdef0123456789abcdef0123456789";

function land(hash: string, parkedState: string | null = STATE) {
  if (parkedState === null) {
    sessionStorage.removeItem(OAUTH_STATE_KEY);
  } else {
    sessionStorage.setItem(OAUTH_STATE_KEY, parkedState);
  }
  window.history.replaceState(null, "", `/${hash}`);
  captureOAuthRedirect();
}

beforeEach(() => {
  sessionStorage.clear();
  window.history.replaceState(null, "", "/");
});

afterEach(() => {
  // The module holds the captured outcome at module scope; drain it so one
  // test's fragment cannot leak into the next.
  takeOAuthRedirect();
});

describe("captureOAuthRedirect", () => {
  it("takes the access token off the URL", () => {
    // The sharpest reason this runs before React mounts: the fragment carries
    // a bearer token, and it must not survive anywhere it could be logged,
    // shared, or restored from history.
    land("#access_token=tok&expires_in=3599&state=" + STATE);
    expect(window.location.hash).toBe("");
    expect(window.location.href).not.toContain("tok");
  });

  it("consumes the parked nonce, so it cannot validate a later fragment", () => {
    land("#access_token=tok&expires_in=3599&state=" + STATE);
    expect(sessionStorage.getItem(OAUTH_STATE_KEY)).toBeNull();
  });

  it("hands over the token exactly once", () => {
    // One-shot because React's StrictMode double-invokes the effect that reads
    // this. A re-readable outcome would push the same token twice and let the
    // second application race the first one's poll.
    land("#access_token=tok&expires_in=3599&state=" + STATE);
    expect(takeOAuthRedirect()).toEqual({
      kind: "token",
      accessToken: "tok",
      expiresAtMs: expect.any(Number),
    });
    expect(takeOAuthRedirect()).toEqual({ kind: "none" });
  });

  it("leaves an ordinary app open completely alone", () => {
    // The commonest path by far — every load that is not a return from
    // Google. It must cost nothing and claim nothing.
    sessionStorage.setItem(OAUTH_STATE_KEY, STATE);
    window.history.replaceState(null, "", "/?demo#some-anchor");
    captureOAuthRedirect();
    expect(takeOAuthRedirect()).toEqual({ kind: "none" });
    // Neither the anchor nor the query is disturbed, and a nonce parked by a
    // flow still in progress survives.
    expect(window.location.hash).toBe("#some-anchor");
    expect(window.location.search).toBe("?demo");
    expect(sessionStorage.getItem(OAUTH_STATE_KEY)).toBe(STATE);
  });

  it("keeps the path and query when it does clear the fragment", () => {
    sessionStorage.setItem(OAUTH_STATE_KEY, STATE);
    window.history.replaceState(null, "", "/?demo#access_token=tok&expires_in=60&state=" + STATE);
    captureOAuthRedirect();
    expect(window.location.search).toBe("?demo");
    expect(window.location.pathname).toBe("/");
    expect(window.location.hash).toBe("");
  });

  it("reports a crafted fragment as an error rather than a token", () => {
    // No nonce was ever parked by this window, so nothing here is ours.
    land("#access_token=stolen&expires_in=3599&state=whatever", null);
    expect(takeOAuthRedirect()).toEqual({ kind: "error", error: "state_mismatch" });
  });
});
