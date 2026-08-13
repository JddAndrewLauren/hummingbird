import { test } from "node:test";
import assert from "node:assert/strict";
import { getSkill } from "../src/skills-registry.js";

test("resolves each shipped skill by name", () => {
  assert.equal(getSkill("parse-capture")?.name, "parse-capture");
  assert.equal(getSkill("next-up-hb")?.name, "next-up-hb");
  assert.equal(getSkill("microtask")?.name, "microtask");
  assert.equal(getSkill("grill-me")?.name, "grill-me");
});

test("returns undefined for an unknown skill name", () => {
  // `next-up-personal` is retired. `to-actions` uses the owned authority
  // interactively but is not a hosted op.
  assert.equal(getSkill("next-up-personal"), undefined);
  assert.equal(getSkill("to-actions"), undefined);
  assert.equal(getSkill("bogus"), undefined);
});
