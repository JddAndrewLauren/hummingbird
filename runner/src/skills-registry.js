import { parseCapture } from "./skills/parse-capture.js";
import { nextUp } from "./skills/next-up-hb.js";

/**
 * The ops this build ships: `parse-capture` (#256) and `next-up-hb` (#116).
 * Adding a skill here is exactly the registration a new op needs; nothing
 * else in `server.js` names a skill directly. `microtask` still waits
 * behind the write-target decision -- both ops here write to nothing.
 */
const SKILLS = new Map([
  [parseCapture.name, parseCapture],
  [nextUp.name, nextUp],
]);

/**
 * @param {string} name
 * @returns {typeof parseCapture | typeof nextUp | undefined}
 */
export function getSkill(name) {
  return SKILLS.get(name);
}
