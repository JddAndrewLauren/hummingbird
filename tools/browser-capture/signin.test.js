// The options page's Sign in with Google (ADR-0040 decision 3, extension
// leg): every decidable step of it, from the URL `launchWebAuthFlow` opens
// to what the authority's answer means.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  GOOGLE_SIGNIN_CLIENT_ID,
  SIGNIN_PATH,
  authUrl,
  defaultDeviceName,
  newNonce,
  parseSigninRedirect,
  signinBody,
  signinOutcome,
} from "./lib.js";

const REDIRECT = "https://eljlbpcfpddadchmlmcabfndadekconp.chromiumapp.org/";

/// A JWT-shaped string whose payload is `claims`. The signature is never
/// read here: the authority verifies it, the extension only checks its
/// own nonce came back.
function idToken(claims) {
  const part = (value) => Buffer.from(JSON.stringify(value)).toString("base64url");
  return `${part({ alg: "RS256", kid: "k1" })}.${part(claims)}.c2ln`;
}

test("the auth URL asks Google for an ID token, for openid email, with this request's nonce", () => {
  const url = new URL(authUrl({ redirectUri: REDIRECT, nonce: "n-123" }));
  assert.equal(url.origin + url.pathname, "https://accounts.google.com/o/oauth2/v2/auth");
  assert.deepEqual(Object.fromEntries(url.searchParams), {
    client_id: GOOGLE_SIGNIN_CLIENT_ID,
    response_type: "id_token",
    scope: "openid email",
    redirect_uri: REDIRECT,
    nonce: "n-123",
  });
  assert.match(url.search, /scope=openid(%20|\+)email/, "the scope is one space-separated parameter");
});

test("a nonce is 16 random bytes as hex, fresh each time", () => {
  assert.equal(newNonce(new Uint8Array([0, 1, 15, 16, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 171])), "00010f10ff00000000000000000000ab");
  const a = newNonce();
  assert.match(a, /^[0-9a-f]{32}$/);
  assert.notEqual(a, newNonce());
});

test("a redirect carrying an ID token with this request's nonce yields the token", () => {
  const token = idToken({ sub: "1", nonce: "n-123" });
  assert.deepEqual(parseSigninRedirect(`${REDIRECT}#id_token=${token}&authuser=0&prompt=none`, "n-123"), { idToken: token });
});

test("a mismatched nonce is refused, and so is a token with none", () => {
  const problem = "Google's answer is not for this sign-in. Sign in again.";
  assert.deepEqual(parseSigninRedirect(`${REDIRECT}#id_token=${idToken({ nonce: "other" })}`, "n-123"), { problem });
  assert.deepEqual(parseSigninRedirect(`${REDIRECT}#id_token=${idToken({ sub: "1" })}`, "n-123"), { problem });
  assert.deepEqual(parseSigninRedirect(`${REDIRECT}#id_token=${idToken({ nonce: "" })}`, ""), { problem }, "an empty expected nonce never matches");
});

test("an error fragment says what Google said, and a cancel says so plainly", () => {
  assert.deepEqual(parseSigninRedirect(`${REDIRECT}#error=access_denied`, "n-123"), { problem: "Sign-in was cancelled." });
  assert.deepEqual(parseSigninRedirect(`${REDIRECT}#error=invalid_request&error_description=redirect_uri_mismatch`, "n-123"), {
    problem: "Google refused the sign-in: invalid_request (redirect_uri_mismatch).",
  });
  assert.deepEqual(parseSigninRedirect(`${REDIRECT}#error=server_error`, "n-123"), { problem: "Google refused the sign-in: server_error." });
});

test("a redirect with no readable ID token is refused", () => {
  assert.deepEqual(parseSigninRedirect(`${REDIRECT}#state=x`, "n"), { problem: "Google's answer carried no ID token." });
  assert.deepEqual(parseSigninRedirect(REDIRECT, "n"), { problem: "Google's answer carried no ID token." });
  assert.deepEqual(parseSigninRedirect(undefined, "n"), { problem: "Google's answer carried no ID token." });
  assert.deepEqual(parseSigninRedirect("not a url", "n"), { problem: "Google's answer carried no ID token." });
  for (const bad of ["onlyone", "a.!!!.c", `a.${Buffer.from("not json").toString("base64url")}.c`, `a.${Buffer.from("7").toString("base64url")}.c`]) {
    assert.deepEqual(parseSigninRedirect(`${REDIRECT}#id_token=${bad}`, "n"), { problem: "Google's ID token could not be read." }, bad);
  }
});

test("the default name is the profile's machine, by its OS", () => {
  assert.equal(defaultDeviceName("mac"), "Mac");
  assert.equal(defaultDeviceName("win"), "Windows PC");
  assert.equal(defaultDeviceName("linux"), "Linux");
  assert.equal(defaultDeviceName("cros"), "Chromebook");
  assert.equal(defaultDeviceName("something-new"), "Browser", "an unknown OS still names the device");
  assert.equal(defaultDeviceName(undefined), "Browser");
});

test("the sign-in body is exactly GoogleSignIn's three keys, as chrome", () => {
  assert.equal(SIGNIN_PATH, "/api/auth/google");
  assert.deepEqual(signinBody({ idToken: "eyJ.x.y", name: "  Mac  " }), { id_token: "eyJ.x.y", name: "Mac", platform: "chrome" });
});

const TOKEN = `hb_${"a".repeat(64)}`;
const json = (status, body) => ({ status, contentType: "application/json", bodyText: JSON.stringify(body) });

test("a 201 carrying a token is signed in", () => {
  assert.deepEqual(signinOutcome(json(201, { id: "device-chrome-mac", token: TOKEN })), {
    kind: "signed_in",
    id: "device-chrome-mac",
    token: TOKEN,
    message: "Signed in as device-chrome-mac.",
  });
});

test("a 201 that is not a token, or a page, is not a sign-in", () => {
  for (const response of [
    json(201, { id: "device-chrome-mac" }),
    json(201, { id: "device-chrome-mac", token: "hb_short" }),
    { status: 201, contentType: "application/json", bodyText: "{" },
    { status: 200, contentType: "text/html", bodyText: "<!doctype html>" },
    { status: 201, contentType: "text/html", bodyText: "<!doctype html>" },
  ]) {
    assert.equal(signinOutcome(response).kind, "misrouted", JSON.stringify(response));
  }
});

test("403, 429 and 503 each say their own thing", () => {
  const forbidden = signinOutcome({ status: 403, contentType: null, bodyText: "" });
  const limited = signinOutcome({ status: 429, contentType: null, bodyText: "" });
  const unconfigured = signinOutcome(json(503, { error: "signin_unconfigured", message: "Google sign-in is not configured on this server." }));
  const unavailable = signinOutcome(json(503, { error: "signin_unavailable", message: "x" }));
  assert.equal(forbidden.kind, "not_allowed");
  assert.equal(limited.kind, "rate_limited");
  assert.equal(unconfigured.kind, "unconfigured");
  assert.equal(unavailable.kind, "unavailable");
  const messages = [forbidden, limited, unconfigured, unavailable].map((o) => o.message);
  assert.equal(new Set(messages).size, 4, "four distinct messages");
  assert.match(forbidden.message, /not the one allowed/);
  assert.match(limited.message, /Too many sign-ins/);
  assert.match(unconfigured.message, /not set up/);
  assert.match(unavailable.message, /try again/i);
  assert.equal(signinOutcome({ status: 503, contentType: "text/plain", bodyText: "upstream" }).kind, "unavailable", "only the explicit code reads as not set up");
});

test("Cloudflare's HTML 403 is the edge, not the allowlist", () => {
  assert.equal(signinOutcome({ status: 403, contentType: "text/html; charset=utf-8", bodyText: "error code: 1010" }).kind, "edge");
});

test("a 400 names the server's reason, anything else its status", () => {
  assert.deepEqual(signinOutcome(json(400, { error: "validation", message: "name must be non-empty" })), {
    kind: "invalid",
    message: "The authority refused the device name: name must be non-empty",
  });
  assert.equal(signinOutcome({ status: 400, contentType: "text/plain", bodyText: "?" }).message, "The authority refused the device name.");
  assert.deepEqual(signinOutcome({ status: 500, contentType: null, bodyText: "" }), { kind: "error", message: "The authority answered 500. Nothing was stored." });
});

// The client id is a copy of the authority's own audience list
// (`GOOGLE_SIGNIN_CLIENT_IDS`, wrangler.toml's `[vars]`): a token minted
// for any other client is a 403 the extension could never get past.
test("the extension signs in through a client the authority accepts", () => {
  const toml = readFileSync(new URL("../../server/worker/wrangler.toml", import.meta.url), "utf8");
  const line = toml.match(/^GOOGLE_SIGNIN_CLIENT_IDS\s*=\s*"([^"]*)"/m);
  assert.ok(line, "wrangler.toml sets GOOGLE_SIGNIN_CLIENT_IDS");
  assert.ok(line[1].split(",").map((id) => id.trim()).includes(GOOGLE_SIGNIN_CLIENT_ID));
});
