import { parseCapture } from "./skills/parse-capture.js";

/**
 * v1 ships `parse-capture` only (#256, 2026-08-10 decision) --
 * `next-up-personal` / `microtask` wait behind the write-target decision.
 * Adding a skill here is exactly the registration a new op needs; nothing
 * else in `server.js` names a skill directly.
 */
const SKILLS = new Map([[parseCapture.name, parseCapture]]);

/**
 * @param {string} name
 * @returns {typeof parseCapture | undefined}
 */
export function getSkill(name) {
  return SKILLS.get(name);
}
