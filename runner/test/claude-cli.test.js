import { test } from "node:test";
import assert from "node:assert/strict";
import { buildClaudeArgs, isValidModelId } from "../src/claude-cli.js";

test("builds the argv the #41 contract names: -p, --output-format json, --json-schema <schema text>", () => {
  const schemaText = '{"type":"object","required":["title"]}';
  const args = buildClaudeArgs("/parse-capture\n\nbuy milk", schemaText);
  assert.deepEqual(args, [
    "-p",
    "/parse-capture\n\nbuy milk",
    "--output-format",
    "json",
    "--json-schema",
    schemaText,
  ]);
});

test("passes the schema's text, never a path -- the CLI rejects a path before it runs anything", () => {
  const args = buildClaudeArgs("p", '{"type":"object"}');
  const schemaArg = args[args.indexOf("--json-schema") + 1];
  assert.doesNotThrow(() => JSON.parse(schemaArg));
});

// --- the `model` arg (#273) ----------------------------------------------

test("no --model when the request asked for none", () => {
  const args = buildClaudeArgs("p", "{}");
  assert.equal(args.includes("--model"), false);
});

test("--model is appended when the request asked for one", () => {
  const args = buildClaudeArgs("p", "{}", "claude-opus-5");
  assert.deepEqual(args.slice(-2), ["--model", "claude-opus-5"]);
  // Appended, never inserted: the schema text stays the element after
  // `--json-schema`.
  assert.equal(args[args.indexOf("--json-schema") + 1], "{}");
});

test("isValidModelId accepts every real id shape", () => {
  for (const id of [
    "sonnet",
    "claude-sonnet-4-5-20250929",
    "kimi-k3",
    "us.anthropic.claude-sonnet-4-5-20250929-v1:0",
  ]) {
    assert.equal(isValidModelId(id), true, id);
  }
});

/**
 * The leading-alphanumeric rule is the security-relevant half: the value
 * becomes a single argv element, and a flag-shaped one would be parsed as
 * an option rather than a value. This must never relax to "any non-empty
 * string".
 */
test("isValidModelId rejects a flag-shaped value", () => {
  for (const id of ["--dangerously-skip-permissions", "-p", "--model", "-"]) {
    assert.equal(isValidModelId(id), false, id);
  }
});

test("isValidModelId rejects the empty, the spaced, the over-long and the non-string", () => {
  assert.equal(isValidModelId(""), false);
  assert.equal(isValidModelId("sonnet 4"), false);
  assert.equal(isValidModelId("a\nb"), false);
  assert.equal(isValidModelId("a".repeat(129)), false);
  assert.equal(isValidModelId("a".repeat(128)), true);
  assert.equal(isValidModelId(undefined), false);
  assert.equal(isValidModelId(null), false);
  assert.equal(isValidModelId(42), false);
  assert.equal(isValidModelId({ toString: () => "sonnet" }), false);
});
