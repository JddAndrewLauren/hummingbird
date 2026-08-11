import { test } from "node:test";
import assert from "node:assert/strict";
import { getSkill } from "../src/skills-registry.js";

test("resolves each shipped skill by name", () => {
  assert.equal(getSkill("parse-capture")?.name, "parse-capture");
  assert.equal(getSkill("next-up-hb")?.name, "next-up-hb");
});

test("returns undefined for an unknown skill name", () => {
  // `next-up-personal` is retired. The migrated `microtask` and `to-actions`
  // skills use the owned authority interactively but are not hosted ops.
  assert.equal(getSkill("next-up-personal"), undefined);
  assert.equal(getSkill("microtask"), undefined);
  assert.equal(getSkill("to-actions"), undefined);
  assert.equal(getSkill("bogus"), undefined);
});
