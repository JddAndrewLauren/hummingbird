import { test } from "node:test";
import assert from "node:assert/strict";
import { resolveBackend, resolveModel } from "../src/stamp.js";

// --- the backend half ----------------------------------------------------

test("an unset ANTHROPIC_BASE_URL is the first-party path", () => {
  assert.equal(resolveBackend({}), "anthropic");
  assert.equal(resolveBackend({ ANTHROPIC_BASE_URL: "" }), "anthropic");
});

test("a third-party base URL stamps its hostname, not the whole URL", () => {
  assert.equal(resolveBackend({ ANTHROPIC_BASE_URL: "https://api.moonshot.ai/anthropic" }), "api.moonshot.ai");
  assert.equal(resolveBackend({ ANTHROPIC_BASE_URL: "https://gateway.example:8443/v1" }), "gateway.example");
});

/**
 * A path, a port or (in a misconfiguration) credentials must not ride onto
 * a line the app renders -- the hostname is all of it.
 */
test("a base URL carrying credentials stamps only the host", () => {
  assert.equal(
    resolveBackend({ ANTHROPIC_BASE_URL: "https://user:secret@proxy.example/v1" }),
    "proxy.example",
  );
});

/**
 * The whole reason this function takes the hostname is that a URL can carry
 * things that must not reach a rendered line. An unparseable string can hold
 * anything, so the fallback must not be the raw value either.
 */
test("a malformed base URL is 'unknown', never echoed back", () => {
  for (const base of ["not a url", "://nope"]) {
    assert.equal(resolveBackend({ ANTHROPIC_BASE_URL: base }), "unknown", base);
  }
});

/** `URL` accepts any `scheme:rest`, so a value like this parses with an
 * empty hostname — which would render as a blank stamp. */
test("a URL that parses with no host is 'unknown', not an empty stamp", () => {
  for (const base of ["user:hunter2@@@", "mailto:someone@example.com", "about:blank"]) {
    assert.equal(resolveBackend({ ANTHROPIC_BASE_URL: base }), "unknown", base);
  }
});

// --- the model half ------------------------------------------------------

test("what the CLI reported it ran wins over everything", () => {
  assert.equal(
    resolveModel({ reported: "claude-opus-5", requested: "sonnet", configured: "kimi-k3" }),
    "claude-opus-5",
  );
});

test("the requested model is next, which is the normal case", () => {
  assert.equal(resolveModel({ requested: "sonnet", configured: "kimi-k3" }), "sonnet");
});

test("the configured model is third", () => {
  assert.equal(resolveModel({ configured: "kimi-k3" }), "kimi-k3");
});

/**
 * The trap the four-step chain exists for: `ANTHROPIC_MODEL` is set only on
 * the third-party provider path, so on the ordinary first-party deployment
 * `configured` is empty and a config-only read would stamp `null` for the
 * most common case.
 */
test("with nothing configured, the requested model still carries the stamp", () => {
  assert.equal(resolveModel({ requested: "sonnet", configured: null }), "sonnet");
});

test("nothing known says nothing rather than guessing", () => {
  assert.equal(resolveModel({}), null);
  assert.equal(resolveModel({ reported: undefined, requested: undefined, configured: null }), null);
});

test("an empty or non-string candidate is skipped, never stamped", () => {
  assert.equal(resolveModel({ reported: "", requested: "sonnet" }), "sonnet");
  assert.equal(resolveModel({ reported: 42, requested: "sonnet" }), "sonnet");
  assert.equal(resolveModel({ requested: { id: "x" }, configured: "kimi-k3" }), "kimi-k3");
});
