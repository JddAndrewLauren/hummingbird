import { test } from "node:test";
import assert from "node:assert/strict";
import { parseCapture } from "../src/skills/parse-capture.js";

test("name matches the skill directory / slash command", () => {
  assert.equal(parseCapture.name, "parse-capture");
});

test("resultSchemaPath points beside the SKILL.md, versioned in this repo", () => {
  assert.equal(parseCapture.resultSchemaPath, ".claude/skills/parse-capture/schema.json");
});

test("validateArgs accepts {text: non-empty string}", () => {
  assert.deepEqual(parseCapture.validateArgs({ text: "buy milk" }), { ok: true });
});

test("validateArgs rejects a missing text field", () => {
  const result = parseCapture.validateArgs({});
  assert.equal(result.ok, false);
  assert.match(result.error, /text/i);
});

test("validateArgs rejects an empty or whitespace-only text field", () => {
  assert.equal(parseCapture.validateArgs({ text: "" }).ok, false);
  assert.equal(parseCapture.validateArgs({ text: "   " }).ok, false);
});

test("validateArgs rejects a non-string text field", () => {
  assert.equal(parseCapture.validateArgs({ text: 5 }).ok, false);
});

test("buildPrompt invokes the /parse-capture slash command and carries the raw text verbatim", () => {
  const prompt = parseCapture.buildPrompt({ text: "call mom  Thursday" });
  assert.match(prompt, /^\/parse-capture\b/);
  assert.match(prompt, /call mom  Thursday/);
});
