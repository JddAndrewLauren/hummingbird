import { test } from "node:test";
import assert from "node:assert/strict";
import { stepId } from "../src/step-id.js";

/**
 * The values below were produced by `deterministic_id` in
 * `.claude/skills/microtask/scripts/hb.sh` -- the bash function this module
 * reimplements -- run over the same two seeds. That is the only thing this
 * file is really for: the interactive arm and the hosted arm write to one
 * `steps` table, and if their ids ever diverged the failure would be silent
 * (a second copy of a checklist the operator already has), so the agreement
 * gets pinned rather than assumed.
 *
 * Regenerate by extracting the function and calling it:
 *
 *   deterministic_id "11111111-2222-4333-8444-555555555555/put on music"
 */
const ITEM = "11111111-2222-4333-8444-555555555555";
const FROM_HB_SH = {
  "put on music": "e3575bf3-23e5-431f-bce9-e540ad31df8f",
  "grab a trash bag": "b28af297-af46-4fdd-a2ab-7c176bf30d04",
};

test("mints the same id hb.sh's deterministic_id does, for the same item and body", () => {
  for (const [body, expected] of Object.entries(FROM_HB_SH)) {
    assert.equal(stepId(ITEM, body), expected);
  }
});

test("is stable across calls -- a replay lands on the authority's already-exists path", () => {
  assert.equal(stepId(ITEM, "put on music"), stepId(ITEM, "put on music"));
});

test("the item and the body both move the id, so no two items share a step row", () => {
  assert.notEqual(stepId(ITEM, "a"), stepId(ITEM, "b"));
  assert.notEqual(stepId(ITEM, "a"), stepId("22222222-2222-4333-8444-555555555555", "a"));
});

test("carries the uuid v4 version and variant nibbles", () => {
  for (const body of ["a", "b", "put on music", "x".repeat(500)]) {
    assert.match(
      stepId(ITEM, body),
      /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
    );
  }
});
