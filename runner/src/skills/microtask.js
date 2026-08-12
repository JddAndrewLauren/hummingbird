import { stepId } from "../step-id.js";

/**
 * The runner's third op (#272): break one already-selected item into a
 * checklist of tiny steps, and land that checklist in the owned `steps`
 * table.
 *
 * **This is the first op that touches the authority**, and the shape of
 * that is the whole of this module. `parse-capture` and `next-up-hb` write
 * to nothing and hold no credential; this one reads the item and appends
 * its steps through the runner's own `device` token (`authority.js`). What
 * it does *not* do is hand that reach to the model: the hosted arm has no
 * shell -- `claude -p` is non-interactive, so a `Bash` call cannot be
 * prompted for and is simply denied, and `claude-cli.js` passes no
 * `--allowedTools` (see `rank-bin.js` for why granting one is the worse
 * trade). So the two halves are split the way `next-up-hb` splits ranking:
 *
 * - `prepare` reads the item and its live steps **before** the model runs,
 *   so an unknown ref is a named envelope error with no tokens spent;
 * - the model's only job is judgment -- the grain, the ramp step, the
 *   wording -- answered against the versioned schema beside SKILL.md;
 * - `apply` performs the writes **after**, one `POST /api/steps` per step,
 *   with ids minted by `step-id.js`.
 *
 * **Idempotence is structural, not a retry policy.** Each step's id is
 * `sha256(namespace + item + body)`, and a create against an existing id is
 * the authority's already-exists path (200, the stored row) rather than a
 * duplicate. Re-running the identical checklist therefore mints nothing --
 * which is what makes an interrupted run safe to simply repeat, and it is
 * the same guarantee `hb.sh` gives the interactive arm because it is
 * literally the same id.
 *
 * **This arm appends and nothing else.** It has no `tick` and no
 * `drop-step`: the refresh rule's "decide what has been superseded" is a
 * reading of the work that stays with the operator's session, and a hosted
 * op that could soft-delete rows would widen this credential's reach for a
 * judgment nobody asked it to make. The already-`done` steps ride in the
 * prompt so the model can *report* them in `note`, which is the half of the
 * refresh rule this arm can honour.
 */

/** SKILL.md's grain scale: 1 coarse, 2 default, 3 max. */
const GRAINS = [1, 2, 3];
const DEFAULT_GRAIN = 2;

function isObject(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/**
 * `HB-42` (case-insensitive) or a bare uuid onto the item row, off a sweep
 * already fetched -- `resolve_ref` in `hb.sh`, in JavaScript. No route
 * accepts `HB-<seq>`: it is a client-side affordance over `Item.seq`, so
 * resolving it is the caller's job on either arm.
 *
 * @param {{items: Array<Record<string, unknown>>}} sweep
 * @param {string} ref
 * @returns {{ok: true, item: Record<string, unknown>} | {ok: false, error: string}}
 */
export function resolveRef(sweep, ref) {
  const seqMatch = /^[Hh][Bb]-(\d+)$/.exec(ref);
  const item = seqMatch
    ? sweep.items.find((candidate) => candidate?.seq === Number(seqMatch[1]))
    : sweep.items.find((candidate) => candidate?.id === ref);

  // An unknown ref is a named failure, never an empty answer written
  // against something else (SKILL.md's failure modes).
  if (!item) return { ok: false, error: `no item ${ref} in the sweep` };
  return { ok: true, item };
}

/**
 * The item's live steps, in position order -- `live_steps` in `hb.sh`.
 * Soft-deleted rows are dropped here rather than shown to the model: they
 * are flagged history, and a checklist that re-proposed one would land on
 * its deleted id and quietly resurrect nothing.
 *
 * @param {{steps: Array<Record<string, unknown>>}} sweep
 * @param {string} itemId
 */
export function liveSteps(sweep, itemId) {
  return sweep.steps
    .filter((step) => step?.item_id === itemId && step?.deleted_at === null)
    .sort((a, b) => a.position - b.position || String(a.id).localeCompare(String(b.id)));
}

export const microtask = {
  name: "microtask",

  /** Versioned beside the SKILL.md this ships with (#41 decision 4). */
  resultSchemaPath: ".claude/skills/microtask/schema.json",

  /**
   * Shape only. Whether the ref names a real item is a question for the
   * sweep, not for a regex -- `prepare` asks it, and answers in the
   * authority's own terms.
   *
   * @param {Record<string, unknown>} args
   * @returns {{ok: true} | {ok: false, error: string}}
   */
  validateArgs(args) {
    if (!isObject(args)) {
      return { ok: false, error: "args must be an object" };
    }
    if (typeof args.ref !== "string" || args.ref.trim().length === 0) {
      return { ok: false, error: '"ref" must be a non-empty item reference (HB-42 or a uuid)' };
    }
    if (args.grain !== undefined && !GRAINS.includes(args.grain)) {
      return { ok: false, error: `"grain" must be one of ${GRAINS.join(", ")} when present` };
    }
    return { ok: true };
  },

  /**
   * The read half, run before the model. A missing token, an unreachable
   * authority and an unknown ref all end the stream here, in the
   * authority's own words and before a single model token is spent -- the
   * same posture `next-up-hb`'s ranker failure has.
   *
   * @param {Record<string, unknown>} args
   * @param {{authority: import("../authority.js").unconfiguredAuthority, onProgress: (message: string) => void}} deps
   */
  async prepare(args, { authority, onProgress }) {
    onProgress(`reading ${args.ref} from the authority`);

    const read = await authority.sweep();
    if (!read.ok) return { ok: false, error: `authority: ${read.error}` };

    // Unnamed by `authority:` on purpose -- the authority answered fine; it
    // is the ref that names nothing.
    const resolved = resolveRef(read.sweep, args.ref);
    if (!resolved.ok) return resolved;

    const steps = liveSteps(read.sweep, resolved.item.id);
    onProgress(
      `item ${resolved.item.id} has ${steps.length} live step${steps.length === 1 ? "" : "s"}`,
    );

    // The raw sweep is dropped on the way through for the reason
    // `next-up-hb`'s prepare drops it: it is the largest thing in reach and
    // nothing downstream reads it.
    return {
      ok: true,
      args: { item: resolved.item, steps, grain: args.grain ?? DEFAULT_GRAIN },
    };
  },

  /**
   * @param {Record<string, unknown>} args as returned by `prepare`
   * @returns {string}
   */
  buildPrompt(args) {
    return [
      "/microtask",
      "",
      "Runner arm: the item and its live steps have ALREADY been read from the",
      "authority and follow as JSON. You have no shell here -- do not run",
      "scripts/hb.sh, and do not write, tick or drop anything yourself. Answer",
      "in the schema; the runner appends your steps to the same `steps` table",
      "hb.sh writes, at positions after the ones you were handed.",
      "",
      JSON.stringify(args),
    ].join("\n");
  },

  /**
   * The write half, run after the model. Appends one step per line of the
   * model's answer, at contiguous positions after whatever is already
   * there, with the deterministic id that makes a replay a no-op.
   *
   * The envelope's `result` is returned **unchanged** -- it is the
   * schema-validated model answer and nothing else (#41 decision 4), so
   * what was written is reported on the progress stream instead of smuggled
   * into a shape the schema does not describe. The rows themselves are the
   * durable record, readable through the authority's own read side.
   *
   * @param {{steps: string[], note?: string}} result
   * @param {{args: Record<string, unknown>, authority: import("../authority.js").unconfiguredAuthority, onProgress: (message: string) => void}} deps
   * @returns {Promise<{ok: true, result: unknown} | {ok: false, error: string}>}
   */
  async apply(result, { args, authority, onProgress }) {
    const bodies = Array.isArray(result?.steps)
      ? result.steps.filter((body) => typeof body === "string" && body.trim().length > 0)
      : [];
    // The schema requires a non-empty list, so this is the CLI having
    // answered outside it -- a named failure, never a silent zero-write
    // success reported as `ok:true`.
    if (bodies.length === 0) {
      return { ok: false, error: "the model returned no steps to write" };
    }

    const itemId = args.item.id;
    const basePosition = args.steps.reduce((max, step) => Math.max(max, step.position), 0) + 1;

    let created = 0;
    let replayed = 0;
    for (const [index, body] of bodies.entries()) {
      const write = await authority.createStep({
        id: stepId(itemId, body),
        item_id: itemId,
        body,
        position: basePosition + index,
      });
      if (!write.ok) return { ok: false, error: `authority: ${write.error}` };
      if (write.created) created += 1;
      else replayed += 1;
      onProgress(`wrote step ${index + 1}/${bodies.length}`);
    }

    onProgress(`${created} step${created === 1 ? "" : "s"} written, ${replayed} already existed`);
    return { ok: true, result };
  },
};
