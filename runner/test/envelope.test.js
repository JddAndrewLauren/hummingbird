import { test } from "node:test";
import assert from "node:assert/strict";
import { progressLine, finalOkLine, finalErrorLine } from "../src/envelope.js";

test("progressLine is one NDJSON line carrying a message and a type", () => {
  const line = progressLine("running claude");
  assert.equal(line.endsWith("\n"), true);
  const parsed = JSON.parse(line);
  assert.deepEqual(parsed, { type: "progress", message: "running claude" });
});

test("finalOkLine carries ok:true, the skill name and the result, nothing else", () => {
  const line = finalOkLine("parse-capture", { title: "t", notes: "n" });
  const parsed = JSON.parse(line);
  assert.deepEqual(parsed, {
    ok: true,
    skill: "parse-capture",
    result: { title: "t", notes: "n" },
  });
  assert.equal(line.endsWith("\n"), true);
});

test("finalErrorLine carries ok:false, the skill name and an error string, no result key", () => {
  const line = finalErrorLine("parse-capture", "claude exited 1");
  const parsed = JSON.parse(line);
  assert.deepEqual(parsed, {
    ok: false,
    skill: "parse-capture",
    error: "claude exited 1",
  });
  assert.equal("result" in parsed, false);
});

test("finalErrorLine works with an unresolved skill name (e.g. unknown skill before dispatch)", () => {
  const line = finalErrorLine(null, "unknown skill: bogus");
  const parsed = JSON.parse(line);
  assert.deepEqual(parsed, { ok: false, skill: null, error: "unknown skill: bogus" });
});

// --- the backend/model stamp (#273) --------------------------------------

/**
 * The presence rule is the contract, and both halves of it matter: an
 * unstamped line must omit the keys rather than send them as `null`, so a
 * client can tell "nothing was attempted" from "we ran but could not name
 * the model".
 */
test("an unstamped terminal line carries neither key", () => {
  for (const line of [finalOkLine("microtask", {}), finalErrorLine("microtask", "boom")]) {
    const parsed = JSON.parse(line);
    assert.equal("backend" in parsed, false, line);
    assert.equal("model" in parsed, false, line);
  }
});

test("a stamped ok line carries the backend and the model", () => {
  const parsed = JSON.parse(
    finalOkLine("microtask", { steps: [] }, { backend: "anthropic", model: "claude-opus-5" }),
  );
  assert.deepEqual(parsed, {
    ok: true,
    skill: "microtask",
    result: { steps: [] },
    backend: "anthropic",
    model: "claude-opus-5",
  });
});

test("a stamped error line carries the backend, with the model possibly null", () => {
  const parsed = JSON.parse(
    finalErrorLine("microtask", "the item already has a plan", { backend: "anthropic", model: null }),
  );
  assert.deepEqual(parsed, {
    ok: false,
    skill: "microtask",
    error: "the item already has a plan",
    backend: "anthropic",
    model: null,
  });
});

/** An absent `model` on the stamp is `null` on the wire, never missing. */
test("a stamp with no model still sends the key", () => {
  const parsed = JSON.parse(finalOkLine("microtask", {}, { backend: "anthropic" }));
  assert.equal(parsed.backend, "anthropic");
  assert.equal(parsed.model, null);
  assert.equal("model" in parsed, true);
});
