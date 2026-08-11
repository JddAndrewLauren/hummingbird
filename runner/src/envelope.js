/**
 * NDJSON line builders for the `POST /run` contract (#41 decision 4): a
 * stream of `{"type":"progress",...}` lines, ending in exactly one
 * `{ok, skill, result, error?}` envelope line. Every line is
 * `JSON.stringify(...) + "\n"` -- newline-delimited is what lets a client
 * read the stream incrementally without a framing protocol, and it is what
 * defeats Fly's 60s idle-connection kill (heartbeats keep bytes moving).
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
 * @returns {string}
 */
export function finalOkLine(skill, result) {
  return JSON.stringify({ ok: true, skill, result }) + "\n";
}

/**
 * @param {string | null} skill
 * @param {string} error
 * @returns {string}
 */
export function finalErrorLine(skill, error) {
  return JSON.stringify({ ok: false, skill, error }) + "\n";
}
