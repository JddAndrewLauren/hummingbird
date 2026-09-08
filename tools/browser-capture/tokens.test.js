// The drift gate for the copied tokens (ADR-0026's shape, as a test): the
// file on disk is exactly what `copy-tokens.js` would write today.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { render, target } from "./copy-tokens.js";

test("design/tokens.css is what copy-tokens.js renders from client/web", () => {
  assert.equal(readFileSync(target, "utf8"), render(), "run `node copy-tokens.js` in tools/browser-capture");
});

test("the copy carries no @font-face and no absolute /fonts path", () => {
  const css = readFileSync(target, "utf8");
  assert.doesNotMatch(css, /@font-face/);
  assert.doesNotMatch(css, /url\(\/fonts/);
  assert.match(css, /--font-sans:/, "the stacks themselves are kept");
});
