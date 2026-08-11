import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { parseCapture } from "../src/skills/parse-capture.js";

const repoRoot = fileURLToPath(new URL("../..", import.meta.url));

test("name matches the skill directory / slash command", () => {
  assert.equal(parseCapture.name, "parse-capture");
});

test("resultSchemaPath points beside the SKILL.md, versioned in this repo", () => {
  assert.equal(parseCapture.resultSchemaPath, ".claude/skills/parse-capture/schema.json");
});

// The schema file lives outside `runner/` but is baked into the same image
// (`runner/Dockerfile`) and passed to `--json-schema` on every real run, so
// these are the runner's own tests, not the skill's.
test("the shipped schema file exists at resultSchemaPath and is valid JSON", () => {
  const raw = readFileSync(`${repoRoot}${parseCapture.resultSchemaPath}`, "utf8");
  assert.doesNotThrow(() => JSON.parse(raw));
});

test("the shipped schema carries no $schema key -- the CLI rejects the 2020-12 ref outright", () => {
  // `claude --json-schema '{"$schema":"https://json-schema.org/draft/2020-12/schema",...}'`
  // fails with `no schema with key or ref "..."` before it runs anything, so
  // a $schema key here is a deploy-time outage, not a style nit.
  const schema = JSON.parse(readFileSync(`${repoRoot}${parseCapture.resultSchemaPath}`, "utf8"));
  assert.equal(schema.$schema, undefined);
});

test("the shipped schema is the {title, notes} object #42 named, closed to extra fields", () => {
  const schema = JSON.parse(readFileSync(`${repoRoot}${parseCapture.resultSchemaPath}`, "utf8"));
  assert.equal(schema.type, "object");
  assert.deepEqual(schema.required, ["title", "notes"]);
  assert.equal(schema.additionalProperties, false);
  assert.deepEqual(Object.keys(schema.properties), ["title", "notes"]);
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
