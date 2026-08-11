import { test } from "node:test";
import assert from "node:assert/strict";
import { buildClaudeArgs } from "../src/claude-cli.js";

test("builds the argv the #41 contract names: -p, --output-format json, --json-schema <path>", () => {
  const args = buildClaudeArgs("/parse-capture\n\nbuy milk", "/app/.claude/skills/parse-capture/schema.json");
  assert.deepEqual(args, [
    "-p",
    "/parse-capture\n\nbuy milk",
    "--output-format",
    "json",
    "--json-schema",
    "/app/.claude/skills/parse-capture/schema.json",
  ]);
});
