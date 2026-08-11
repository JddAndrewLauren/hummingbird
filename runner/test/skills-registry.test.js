import { test } from "node:test";
import assert from "node:assert/strict";
import { getSkill } from "../src/skills-registry.js";

test("resolves each shipped skill by name", () => {
  assert.equal(getSkill("parse-capture")?.name, "parse-capture");
  assert.equal(getSkill("next-up-hb")?.name, "next-up-hb");
});

test("returns undefined for an unknown skill name", () => {
  // `next-up-personal` is the Linear-era selector and is deliberately not
  // an op here -- `next-up-hb` (#116) is the owned-authority one.
  assert.equal(getSkill("next-up-personal"), undefined);
  assert.equal(getSkill("bogus"), undefined);
});
