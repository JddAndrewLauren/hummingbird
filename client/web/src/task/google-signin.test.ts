import { describe, expect, it } from "vitest";
import {
  authorizationUrl,
  beginGoogleSignIn,
  deviceNameFromUserAgent,
  NONCE_KEY,
  parseSignInFragment,
  readSignInReturn,
  signInFailureMessage,
} from "./google-signin";
import type { StorageLike } from "../screens/storage";

function memoryStorage(initial: Record<string, string> = {}): StorageLike & { data: Record<string, string> } {
  const data = { ...initial };
  return {
    data,
    getItem: (key) => (key in data ? data[key] : null),
    setItem: (key, value) => {
      data[key] = value;
    },
    removeItem: (key) => {
      delete data[key];
    },
  };
}

function base64url(text: string): string {
  return btoa(text).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

/** An unsigned JWT-shaped string — the client never verifies a signature
 * (the authority does); it only reads the `nonce` claim back out. */
function idToken(claims: Record<string, unknown>): string {
  return `${base64url('{"alg":"RS256","kid":"k"}')}.${base64url(JSON.stringify(claims))}.c2ln`;
}

describe("authorizationUrl", () => {
  it("asks Google for an ID token for openid email, with the nonce and the /signin return", () => {
    const url = new URL(authorizationUrl("client-1.apps.googleusercontent.com", "https://hb.twinion.net/signin", "n0nce"));
    expect(url.origin + url.pathname).toBe("https://accounts.google.com/o/oauth2/v2/auth");
    expect(url.searchParams.get("client_id")).toBe("client-1.apps.googleusercontent.com");
    expect(url.searchParams.get("redirect_uri")).toBe("https://hb.twinion.net/signin");
    expect(url.searchParams.get("response_type")).toBe("id_token");
    expect(url.searchParams.get("scope")).toBe("openid email");
    expect(url.searchParams.get("nonce")).toBe("n0nce");
  });
});

describe("beginGoogleSignIn", () => {
  it("keeps a fresh nonce in session storage and sends it in the URL", () => {
    const storage = memoryStorage();
    const url = new URL(beginGoogleSignIn(storage, "client-1", "http://127.0.0.1:5181"));
    const nonce = storage.data[NONCE_KEY];
    expect(nonce).toMatch(/^[0-9a-f]{32}$/);
    expect(url.searchParams.get("nonce")).toBe(nonce);
    expect(url.searchParams.get("redirect_uri")).toBe("http://127.0.0.1:5181/signin");
  });

  it("never reuses a nonce across two attempts", () => {
    const storage = memoryStorage();
    beginGoogleSignIn(storage, "client-1", "https://hb.twinion.net");
    const first = storage.data[NONCE_KEY];
    beginGoogleSignIn(storage, "client-1", "https://hb.twinion.net");
    expect(storage.data[NONCE_KEY]).not.toBe(first);
  });
});

describe("parseSignInFragment", () => {
  it("accepts an ID token whose nonce matches the one kept", () => {
    const token = idToken({ sub: "1", nonce: "abc" });
    expect(parseSignInFragment(`#id_token=${token}&authuser=0`, "abc")).toEqual({
      kind: "token",
      idToken: token,
    });
  });

  it("rejects a token whose nonce does not match", () => {
    const result = parseSignInFragment(`#id_token=${idToken({ nonce: "other" })}`, "abc");
    expect(result.kind).toBe("error");
  });

  it("rejects a token that carries no nonce at all", () => {
    const result = parseSignInFragment(`#id_token=${idToken({ sub: "1" })}`, "abc");
    expect(result.kind).toBe("error");
  });

  it("rejects any token when no nonce was kept (a replayed or stale return)", () => {
    const result = parseSignInFragment(`#id_token=${idToken({ nonce: "abc" })}`, null);
    expect(result).toEqual({
      kind: "error",
      message: "This sign-in link was already used or has expired. Sign in again.",
    });
  });

  it("rejects a fragment with no token", () => {
    expect(parseSignInFragment("", "abc").kind).toBe("error");
    expect(parseSignInFragment("#authuser=0", "abc").kind).toBe("error");
  });

  it("rejects a token that is not a readable JWT", () => {
    expect(parseSignInFragment("#id_token=not-a-jwt", "abc").kind).toBe("error");
    expect(parseSignInFragment("#id_token=a.%%%.c", "abc").kind).toBe("error");
  });

  it("words an error fragment, a cancelled consent included", () => {
    expect(parseSignInFragment("#error=access_denied", "abc")).toEqual({
      kind: "error",
      message: "Sign-in was cancelled.",
    });
    const other = parseSignInFragment("#error=invalid_request&error_description=Bad+redirect", "abc");
    expect(other).toEqual({ kind: "error", message: "Google refused the sign-in: Bad redirect" });
    expect(parseSignInFragment("#error=server_error", "abc")).toEqual({
      kind: "error",
      message: "Google refused the sign-in: server_error",
    });
  });
});

describe("readSignInReturn", () => {
  it("consumes the kept nonce, so the same return read twice is refused the second time", () => {
    const storage = memoryStorage({ [NONCE_KEY]: "abc" });
    const hash = `#id_token=${idToken({ nonce: "abc" })}`;
    expect(readSignInReturn(hash, storage).kind).toBe("token");
    expect(storage.data[NONCE_KEY]).toBeUndefined();
    expect(readSignInReturn(hash, storage).kind).toBe("error");
  });

  it("consumes the nonce on a failed return too", () => {
    const storage = memoryStorage({ [NONCE_KEY]: "abc" });
    expect(readSignInReturn("#error=access_denied", storage).kind).toBe("error");
    expect(storage.data[NONCE_KEY]).toBeUndefined();
  });
});

describe("deviceNameFromUserAgent", () => {
  it.each([
    [
      "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36",
      "Chrome on macOS",
    ],
    [
      "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36 Edg/141.0.0.0",
      "Edge on Windows",
    ],
    ["Mozilla/5.0 (X11; Linux x86_64; rv:131.0) Gecko/20100101 Firefox/131.0", "Firefox on Linux"],
    [
      "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1",
      "Safari on iOS",
    ],
    [
      "Mozilla/5.0 (Linux; Android 15; Pixel 9) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Mobile Safari/537.36",
      "Chrome on Android",
    ],
    [
      "Mozilla/5.0 (X11; CrOS x86_64 14541.0.0) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36",
      "Chrome on ChromeOS",
    ],
  ])("names %s", (userAgent, name) => {
    expect(deviceNameFromUserAgent(userAgent)).toBe(name);
  });

  it("falls back to a plain name for a browser it cannot place", () => {
    expect(deviceNameFromUserAgent("")).toBe("Web browser");
    expect(deviceNameFromUserAgent("curl/8.0")).toBe("Web browser");
  });
});

describe("signInFailureMessage", () => {
  it("words every refusal the core can answer", () => {
    expect(signInFailureMessage("not_allowed", null)).toBe(
      "This Google account is not allowed to sign in to hummingbird.",
    );
    expect(signInFailureMessage("rate_limited", null)).toMatch(/too many sign-ins/i);
    expect(signInFailureMessage("unconfigured", null)).toMatch(/not set up/i);
    expect(signInFailureMessage("unavailable", null)).toMatch(/try again/i);
    expect(signInFailureMessage("invalid", "name must be non-empty")).toContain("name must be non-empty");
    expect(signInFailureMessage("failed", "connection refused")).toContain("connection refused");
    expect(signInFailureMessage("busy", null)).toMatch(/try again/i);
  });
});
