/**
 * The envelope's `backend` and `model` stamp (#273).
 *
 * A caller reading a checklist needs to know what produced it, and #274
 * makes the answer vary — so the stamp is carried on the wire rather than
 * assumed at the render site. Both halves are decided here, off the
 * environment and the run, never off a hardcoded name.
 */

/**
 * Which provider this process is pointed at. `ANTHROPIC_BASE_URL` is what
 * `runner/scripts/switch-provider.sh` sets to move off the first-party
 * path, so its hostname is the honest label; unset means first-party
 * Anthropic.
 *
 * A hostname, not the whole URL: the URL can carry a path, a port and
 * (in a misconfiguration) credentials, none of which belong on a line the
 * app renders.
 *
 * @param {Record<string, string | undefined>} env
 * @returns {string}
 */
export function resolveBackend(env) {
  const base = env.ANTHROPIC_BASE_URL;
  if (!base) return "anthropic";
  try {
    return new URL(base).hostname;
  } catch {
    // A malformed base URL is the operator's problem, not a reason to
    // refuse a stamp — name it verbatim and let the value be its own
    // evidence.
    return base;
  }
}

/**
 * Which model actually ran, in four steps:
 *
 * 1. **what the CLI reported it ran** — the only source that cannot be
 *    wrong;
 * 2. **the `model` arg the request asked for** — right whenever the CLI
 *    honoured it, which is the normal case;
 * 3. **`ANTHROPIC_MODEL`** — the configured default;
 * 4. **`null`** — say nothing rather than guess.
 *
 * The order matters because of a trap: `ANTHROPIC_MODEL` is set **only on
 * the third-party provider path** (`docs/runner.md`'s provider swap); the
 * first-party Anthropic path sets `ANTHROPIC_API_KEY` and no model at all.
 * "Read the model from config" alone would therefore stamp `null` for the
 * most common deployment.
 *
 * Note what step 2 means on a `prepare` decline, where no model token was
 * ever spent: the stamp names what *would* have run. That is deliberate —
 * `backend` and `model` describe the lane the request took, and a decline
 * with no lane named reads as a different kind of failure than it is.
 *
 * @param {object} sources
 * @param {string} [sources.reported] what the CLI said it ran, if anything
 * @param {unknown} [sources.requested] the request's `model` arg
 * @param {string | null} [sources.configured] `ANTHROPIC_MODEL`
 * @returns {string | null}
 */
export function resolveModel({ reported, requested, configured }) {
  for (const candidate of [reported, requested, configured]) {
    if (typeof candidate === "string" && candidate.length > 0) return candidate;
  }
  return null;
}
