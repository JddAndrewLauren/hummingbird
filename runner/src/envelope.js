/**
 * NDJSON line builders for the `POST /run` contract (#41 decision 4): a
 * stream of `{"type":"progress",...}` lines, ending in exactly one
 * `{ok, skill, result, error?}` envelope line. Every line is
 * `JSON.stringify(...) + "\n"` -- newline-delimited is what lets a client
 * read the stream incrementally without a framing protocol, and it is what
 * defeats Fly's 60s idle-connection kill (heartbeats keep bytes moving).
 *
 * **The terminal line carries a `backend`/`model` stamp when there is one
 * to carry** (#273): whichever provider and model produced the answer, so a
 * client renders what actually ran instead of a name it hardcoded. The
 * presence rule is the whole contract:
 *
 * - an **ok** line always carries both;
 * - a **pipeline error** carries `backend` always and `model` possibly
 *   `null` (`stamp.js` explains what a `prepare` decline stamps);
 * - a **pre-dispatch** failure -- malformed JSON, an unknown skill, bad
 *   args, a too-large body -- carries **neither key**, because nothing was
 *   attempted. Those callers pass no `stamp` at all, and the keys are
 *   omitted rather than sent as `null`, so "absent" and "unknown" stay
 *   distinguishable on the wire.
 *
 * The pre-dispatch failures route through [`finalErrorLine`] too, so there
 * is exactly one builder of a terminal line in this process.
 */

/**
 * @typedef {{backend: string, model: string | null}} Stamp
 */

/**
 * @param {string} message
 * @returns {string}
 */
export function progressLine(message) {
  return JSON.stringify({ type: "progress", message }) + "\n";
}

/**
 * @param {string} skill
 * @param {unknown} result
 * @param {Stamp} [stamp]
 * @returns {string}
 */
export function finalOkLine(skill, result, stamp) {
  return JSON.stringify({ ok: true, skill, result, ...stampKeys(stamp) }) + "\n";
}

/**
 * @param {string | null} skill
 * @param {string} error
 * @param {Stamp} [stamp]
 * @returns {string}
 */
export function finalErrorLine(skill, error, stamp) {
  return JSON.stringify({ ok: false, skill, error, ...stampKeys(stamp) }) + "\n";
}

function stampKeys(stamp) {
  if (!stamp) return {};
  return { backend: stamp.backend, model: stamp.model ?? null };
}
