/**
 * Parses and shape-validates a `POST /run` body against the `{skill, args}`
 * contract (#41 decision 4). Never throws -- every failure is a tagged
 * `{ok:false, error}` the caller turns into a 400, never an uncaught
 * exception that would 500 without a body.
 *
 * @param {string} rawBody
 * @returns {{ok: true, skill: string, args: Record<string, unknown>} | {ok: false, error: string}}
 */
export function parseRunRequest(rawBody) {
  let parsed;
  try {
    parsed = JSON.parse(rawBody);
  } catch {
    return { ok: false, error: "invalid JSON body" };
  }

  if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
    return { ok: false, error: "body must be a JSON object" };
  }

  if (typeof parsed.skill !== "string" || parsed.skill.length === 0) {
    return { ok: false, error: "\"skill\" must be a non-empty string" };
  }

  if (
    parsed.args === undefined ||
    parsed.args === null ||
    typeof parsed.args !== "object" ||
    Array.isArray(parsed.args)
  ) {
    return { ok: false, error: "\"args\" must be an object" };
  }

  return { ok: true, skill: parsed.skill, args: parsed.args };
}
