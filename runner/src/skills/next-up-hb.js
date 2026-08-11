/**
 * The runner's second op (#116): pick what to do right now.
 *
 * **Context-blind, per #41.** The sweep payload arrives in `args`, from the
 * calling device's own mirror -- this runner holds no authority token and
 * makes no HTTP call, exactly as `parse-capture` writes to nothing. The
 * interactive arm of the skill is the one that fetches
 * (`scripts/next-up.sh survey`).
 *
 * **The runner arm ranks BEFORE the model runs**, which is the one place
 * its shape departs from `parse-capture`. `prepare` spawns the prebuilt
 * `next-up-rank` (`rank-bin.js`) and puts the *ranked* JSON in the prompt,
 * so the model's whole job here is the judgment half `SKILL.md` reserves
 * for it -- the axis reading, the one-line why, the fog reading, the
 * writing -- and it needs no tool at all. Asking it to run
 * `scripts/next-up.sh rank` instead would need a `Bash` grant that
 * `claude-cli.js` does not pass and that non-interactive `claude -p`
 * cannot prompt for; see `rank-bin.js` for why granting one is the worse
 * trade. A ranking failure is an envelope `error` before any model token
 * is spent, rather than prose from a model narrating a failed command.
 *
 * Read-only either way: v1 of `/next-up-hb` touches no write route.
 */

/** The owned schema's own spellings -- `hummingbird_domain::{Energy, Size}`. */
const ENERGIES = ["low", "medium", "high"];
const SIZES = ["quick", "short", "deep"];

function isObject(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export const nextUp = {
  name: "next-up-hb",

  /** Versioned beside the SKILL.md this ships with (#41 decision 4). */
  resultSchemaPath: ".claude/skills/next-up-hb/schema.json",

  /**
   * Validates the envelope's shape, and only its shape. Deliberately no
   * check of the `calendar` block's `status`/`event` pairing: the ranker
   * names that problem itself (`EnvelopeProblem`), and a second copy here
   * is a rule that can drift from the one that actually decides.
   *
   * @param {Record<string, unknown>} args
   * @returns {{ok: true} | {ok: false, error: string}}
   */
  validateArgs(args) {
    if (!isObject(args)) {
      return { ok: false, error: "args must be an object" };
    }

    const sweep = args.sweep;
    if (!isObject(sweep) || !Array.isArray(sweep.items)) {
      return { ok: false, error: '"sweep" must be a GET /api/sweep payload (an object with "items")' };
    }

    const now = args.now;
    if (!isObject(now)) {
      return { ok: false, error: '"now" must be an object' };
    }
    if (typeof now.local !== "string" || now.local.trim().length === 0) {
      return { ok: false, error: '"now.local" must be a non-empty naive-local timestamp' };
    }
    if (!Number.isInteger(now.epoch_ms)) {
      return { ok: false, error: '"now.epoch_ms" must be an integer' };
    }

    if (args.axes !== undefined) {
      if (!isObject(args.axes)) {
        return { ok: false, error: '"axes" must be an object when present' };
      }
      const { context, energy, size } = args.axes;
      if (context !== undefined && typeof context !== "string") {
        return { ok: false, error: '"axes.context" must be a string when present' };
      }
      if (energy !== undefined && !ENERGIES.includes(energy)) {
        return { ok: false, error: `"axes.energy" must be one of ${ENERGIES.join(", ")}` };
      }
      if (size !== undefined && !SIZES.includes(size)) {
        return { ok: false, error: `"axes.size" must be one of ${SIZES.join(", ")}` };
      }
    }

    return { ok: true };
  },

  /**
   * Runs the deterministic half. `server.js` calls this after
   * `validateArgs` and before `buildPrompt`, and a `{ok: false}` here
   * terminates the stream with the ordinary envelope -- no `claude` is
   * spawned at all.
   *
   * The returned object is what `buildPrompt` is then handed, so the
   * prompt carries `ranked` rather than the raw sweep. The raw `sweep` is
   * deliberately dropped on the way through: it is the largest thing in
   * the request and every fact the model needs about it is already in the
   * ranker's answer (`candidates` are whole `Item`s, `health` is the
   * footer's material), so forwarding both would spend context on a
   * payload nothing reads.
   *
   * @param {Record<string, unknown>} args
   * @param {{runRanker: (envelope: unknown) => Promise<{ok: true, ranked: unknown} | {ok: false, error: string}>}} deps
   * @returns {Promise<{ok: true, args: Record<string, unknown>} | {ok: false, error: string}>}
   */
  async prepare(args, { runRanker }) {
    const outcome = await runRanker(args);
    if (!outcome.ok) {
      return { ok: false, error: `ranker: ${outcome.error}` };
    }
    const { sweep, ...rest } = args;
    return { ok: true, args: { ...rest, ranked: outcome.ranked } };
  },

  /**
   * The ranked answer rides in the prompt because it is the only channel a
   * `claude -p` run has -- the runner arm has no credential to fetch with
   * and no side channel to write. One routing line, then the JSON; every
   * rule about what to do with it lives in SKILL.md.
   *
   * @param {Record<string, unknown>} args as returned by `prepare`
   * @returns {string}
   */
  buildPrompt(args) {
    return [
      "/next-up-hb",
      "",
      "Runner arm: the ranker has ALREADY run. Its answer follows as JSON,",
      "under `ranked`. Do not re-rank, re-filter or re-sort it, and do not",
      "run scripts/next-up.sh -- you have no shell here.",
      "",
      JSON.stringify(args),
    ].join("\n");
  },
};
