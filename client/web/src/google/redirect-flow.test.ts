import { describe, expect, it } from "vitest";
import { CALENDAR_READONLY_SCOPE } from "./gis";
import { buildAuthorizeUrl, createState, parseRedirectFragment } from "./redirect-flow";

const CLIENT_ID = "client-123.apps.googleusercontent.com";
const REDIRECT_URI = "https://hb.twinion.net/";

describe("buildAuthorizeUrl", () => {
  it("asks for a token, for exactly the read-only calendar scope", () => {
    const url = new URL(buildAuthorizeUrl({ clientId: CLIENT_ID, redirectUri: REDIRECT_URI, state: "abc" }));

    expect(url.origin + url.pathname).toBe("https://accounts.google.com/o/oauth2/v2/auth");
    expect(url.searchParams.get("response_type")).toBe("token");
    // The first acceptance criterion of the whole calendar lane: no write
    // scope, ever. A redirect flow is a second place that could break it.
    expect(url.searchParams.get("scope")).toBe(CALENDAR_READONLY_SCOPE);
    expect(url.searchParams.get("scope")).not.toContain("calendar.events");
    expect(url.searchParams.get("client_id")).toBe(CLIENT_ID);
    expect(url.searchParams.get("state")).toBe("abc");
  });

  it("sends the redirect URI byte-for-byte, trailing slash included", () => {
    // Google matches registered redirect URIs exactly. Dropping or adding the
    // trailing slash is a redirect_uri_mismatch and no amount of app-side
    // handling recovers it.
    const url = new URL(buildAuthorizeUrl({ clientId: CLIENT_ID, redirectUri: REDIRECT_URI, state: "abc" }));
    expect(url.searchParams.get("redirect_uri")).toBe(REDIRECT_URI);
  });
});

describe("parseRedirectFragment", () => {
  const NOW = 1_000_000;

  it("reads an ordinary app open as no redirect at all", () => {
    // The common case, on every single load. Anything else here would report
    // a connection failure to someone who never pressed Connect.
    expect(parseRedirectFragment("", "s", NOW)).toEqual({ kind: "none" });
    expect(parseRedirectFragment("#", "s", NOW)).toEqual({ kind: "none" });
    expect(parseRedirectFragment("#section-heading", "s", NOW)).toEqual({ kind: "none" });
  });

  it("returns the token with an absolute expiry computed from the injected now", () => {
    const outcome = parseRedirectFragment(
      "#access_token=tok&token_type=Bearer&expires_in=3599&state=s",
      "s",
      NOW,
    );
    expect(outcome).toEqual({ kind: "token", accessToken: "tok", expiresAtMs: NOW + 3_599_000 });
  });

  it("refuses a token whose state does not match the one this window parked", () => {
    expect(parseRedirectFragment("#access_token=tok&expires_in=3599&state=other", "s", NOW)).toEqual({
      kind: "error",
      error: "state_mismatch",
    });
    expect(parseRedirectFragment("#access_token=tok&expires_in=3599", "s", NOW)).toEqual({
      kind: "error",
      error: "state_mismatch",
    });
  });

  it("refuses anything at all when this window never started a flow", () => {
    // A fragment arriving with no parked state is not ours — a shared link, a
    // back-navigation into a stale URL, or somebody else's idea.
    expect(parseRedirectFragment("#access_token=tok&expires_in=3599&state=s", null, NOW)).toEqual({
      kind: "error",
      error: "state_mismatch",
    });
  });

  // An attacker-supplied fragment can carry `error=` as easily as a token, and
  // an unchecked error path lets a third party put words in the app's mouth.
  it("checks state on the error path too", () => {
    expect(parseRedirectFragment("#error=access_denied&state=wrong", "s", NOW)).toEqual({
      kind: "error",
      error: "state_mismatch",
    });
    expect(parseRedirectFragment("#error=access_denied&state=s", "s", NOW)).toEqual({
      kind: "error",
      error: "access_denied",
    });
  });

  it("refuses a token with no usable lifetime", () => {
    // Worse than no token: the rotation timer would be scheduled off garbage
    // and the poll would 401 with nothing watching.
    for (const fragment of [
      "#access_token=tok&state=s",
      "#access_token=tok&expires_in=&state=s",
      "#access_token=tok&expires_in=nope&state=s",
      "#access_token=tok&expires_in=0&state=s",
      "#access_token=tok&expires_in=-5&state=s",
    ]) {
      expect(parseRedirectFragment(fragment, "s", NOW)).toEqual({ kind: "error", error: "no_expiry" });
    }
  });

  it("tolerates a fragment that has already lost its leading hash", () => {
    expect(parseRedirectFragment("access_token=tok&expires_in=60&state=s", "s", NOW)).toEqual({
      kind: "token",
      accessToken: "tok",
      expiresAtMs: NOW + 60_000,
    });
  });
});

describe("createState", () => {
  it("is hex over every byte it was given, at full length", () => {
    const state = createState((array) => {
      array.fill(0xab);
      return array;
    });
    expect(state).toBe("ab".repeat(16));
  });

  it("does not truncate leading-zero bytes", () => {
    // `toString(16)` on 0x05 is "5"; without the pad the nonce silently loses
    // entropy, and it is the only thing standing between the app and a
    // fragment somebody else chose.
    const state = createState((array) => {
      array.fill(0x05);
      return array;
    });
    expect(state).toBe("05".repeat(16));
    expect(state).toHaveLength(32);
  });
});
