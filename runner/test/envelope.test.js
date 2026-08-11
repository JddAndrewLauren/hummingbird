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
