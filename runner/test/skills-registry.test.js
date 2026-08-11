import { test } from "node:test";
import assert from "node:assert/strict";
import { getSkill } from "../src/skills-registry.js";

test("resolves the known v1 skill by name", () => {
  const skill = getSkill("parse-capture");
  assert.equal(skill?.name, "parse-capture");
});

test("returns undefined for an unknown skill name", () => {
  assert.equal(getSkill("next-up-personal"), undefined);
  assert.equal(getSkill("bogus"), undefined);
});
