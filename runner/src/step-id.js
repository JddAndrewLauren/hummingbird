import { createHash } from "node:crypto";

/**
 * The step id `/microtask` mints, in JavaScript.
 *
 * **This is `deterministic_id` from `.claude/skills/microtask/scripts/hb.sh`,
 * digit for digit, and the two must never diverge**: the interactive arm and
 * this hosted one write to the same `steps` table, so the same item and the
 * same step text have to land on the same row whichever arm ran. Divergence
 * would not fail loudly -- it would quietly mint a second copy of a
 * checklist the operator already has. `runner/test/step-id.test.js` pins the
 * agreement against the value the bash function actually produces.
 *
 * The recipe: `sha256(namespace + seed)`, first 16 bytes, with the version
 * and variant nibbles forced into UUID v4 shape -- the same shape
 * `client/core/src/sync/write/id.rs` and `sweep.py` use, in their own
 * hash domains.
 */

/**
 * Frozen. Changing it re-mints every step this skill has ever written, on
 * both arms.
 */
const ID_NAMESPACE = "hummingbird-skill/microtask/v1";

/**
 * The seed carries the item and the body but **not** the position, so
 * re-running a checklist whose steps shifted down by one does not mint a
 * second copy of any of them.
 *
 * @param {string} itemId the item's uuid
 * @param {string} body the step's text, exactly as it will be written
 * @returns {string} a uuid
 */
export function stepId(itemId, body) {
  const digest = createHash("sha256")
    .update(`${ID_NAMESPACE}${itemId}/${body}`)
    .digest("hex")
    .slice(0, 32);

  const byte6 = ((parseInt(digest.slice(12, 14), 16) & 0x0f) | 0x40).toString(16).padStart(2, "0");
  const byte8 = ((parseInt(digest.slice(16, 18), 16) & 0x3f) | 0x80).toString(16).padStart(2, "0");
  const hex =
    digest.slice(0, 12) + byte6 + digest.slice(14, 16) + byte8 + digest.slice(18, 32);

  return [
    hex.slice(0, 8),
    hex.slice(8, 12),
    hex.slice(12, 16),
    hex.slice(16, 20),
    hex.slice(20, 32),
  ].join("-");
}
