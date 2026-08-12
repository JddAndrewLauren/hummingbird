import { parseCapture } from "./skills/parse-capture.js";
import { nextUp } from "./skills/next-up-hb.js";
import { microtask } from "./skills/microtask.js";

/**
 * The ops this build ships: `parse-capture` (#256), `next-up-hb` (#116) and
 * `microtask` (#272). Adding a skill here is exactly the registration a new
 * op needs; nothing else in `server.js` names a skill directly.
 * `to-actions` uses the app-owned authority interactively but is not a
 * hosted op.
 */
const SKILLS = new Map([
  [parseCapture.name, parseCapture],
  [nextUp.name, nextUp],
  [microtask.name, microtask],
]);

/**
 * @param {string} name
 * @returns {typeof parseCapture | typeof nextUp | typeof microtask | undefined}
 */
export function getSkill(name) {
  return SKILLS.get(name);
}
