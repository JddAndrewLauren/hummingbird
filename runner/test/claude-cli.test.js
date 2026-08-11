import { test } from "node:test";
import assert from "node:assert/strict";
import { buildClaudeArgs } from "../src/claude-cli.js";

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
