import { isValidModelId } from "../claude-cli.js";
import { resolveRef } from "./microtask.js";

/**
 * The runner's fourth op (#350): the item-scoped interview, one typed turn
 * at a time. This is deliberately the whole of the Grill plan's first slice
 * (#349) -- no schema, no client, no ADR, all of which wait on #351's live
 * measurement session. By the end of this slice the interview can be driven
 * end-to-end by `curl` against `POST /run`, with the caller threading the
 * transcript by hand.
 *
 * **Stateless, like `next-up-hb`, and read-only like both `parse-capture`
 * and `next-up-hb` -- never like `microtask`.** Every request carries the
 * whole conversation so far (`turns`); there is no session on this process
 * and no row anywhere that remembers a transcript between requests. This
 * op has **no `apply`** and calls no write method on `../authority.js` --
 * `sweep()` is the only authority call it ever makes. A future slice may
 * persist a confirmed grill (#353's `grills` table), but that is a
 * different op's job; this one only ever answers with a turn.
 *
 * **`prepare` is the only read, and it runs before a model token is
 * spent** -- the same posture `microtask`'s `prepare` has for an unknown
 * ref, a missing token or an unreachable authority. Three things end the
 * stream here: the ref not resolving, the process holding no authority
 * token (`unconfiguredAuthority`'s named error), or the authority being
 * unreachable.
 *
 * **The turn cap belongs here, never in the UI** -- the same posture as
 * #312's live-plan decline for `microtask`. `PROVISIONAL_TURN_CAP` is a
 * placeholder: #351's live-run measurement sets the real number, and until
 * then this one exists purely so an interview cannot run away against a
 * scale-to-zero, metered backend.
 *
 * **Prior *applied* grill outcomes reach the prompt; prior transcripts
 * never do** (#349's decision 3, read forward from a table that does not
 * exist yet). `pastOutcomes` reads an optional `sweep.grills` defensively
 * -- nothing in `server/domain` populates it today, so in production this
 * is always `[]`. The field mapping is written against #353's *specified*
 * column list (`id, item_id, transcript, summary, verdict,
 * model_proposal, applied_patch, resulting_stage, completed_at, version`)
 * but is unverified until that slice actually lands -- only `summary`,
 * `verdict` and `applied_patch` are lifted out (as `patch`), and
 * `model_proposal` is deliberately left out of the prompt for this slice
 * (see `pastOutcomes`'s own doc comment for why); a `transcript` key is
 * dropped on the way through and never reaches the model.
 */

/**
 * Provisional (#350's brief, restated): the gate slice (#351) sets the real
 * number from a live-run measurement. Ship it low rather than unset --
 * an interview with no cap at all is the runaway-cost failure #312 named
 * for `microtask`'s bare-run case, transplanted to a per-turn cost instead
 * of a per-write one.
 */
export const PROVISIONAL_TURN_CAP = 8;

const MIN_CHOICES = 2;
const MAX_CHOICES = 4;

function isObject(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isNonEmptyString(value) {
  return typeof value === "string" && value.trim().length > 0;
}

/**
 * The shape a prior turn's `question` half must have -- exactly the
 * schema's own `question` branch, checked here too because a malformed
 * prior turn (one a caller threads back from a run that somehow answered
 * outside the schema) must be a named 400 at `validateArgs`, never a value
 * quietly forwarded into the next prompt.
 *
 * @param {unknown} value
 * @returns {boolean}
 */
function isQuestionShape(value) {
  if (!isObject(value)) return false;
  if (!isNonEmptyString(value.prompt)) return false;
  if (!isNonEmptyString(value.recommendedAnswer)) return false;
  if (!Array.isArray(value.choices)) return false;
  if (value.choices.length < MIN_CHOICES || value.choices.length > MAX_CHOICES) return false;
  return value.choices.every((choice) => isNonEmptyString(choice));
}

/**
 * One round of the conversation so far: the question the model asked, and
 * the caller's answer to it -- free text always, never constrained to
 * `question.choices` (the schema's own choices are a recommendation
 * surfaced to the human, not a closed set the answer must belong to).
 *
 * @param {unknown} value
 * @returns {boolean}
 */
function isTurnShape(value) {
  if (!isObject(value)) return false;
  if (!isQuestionShape(value.question)) return false;
  return isNonEmptyString(value.answer);
}

/**
 * Prior *applied* grill outcomes for this item -- never a past transcript
 * (#349 decision 3). `sweep.grills` is optional and, today, never
 * populated (no route mints it yet). The field mapping below is written
 * against #353's *specified* row columns (`id, item_id, transcript,
 * summary, verdict, model_proposal, applied_patch, resulting_stage,
 * completed_at, version`) and is unverified until that slice actually
 * lands -- reading defensively rather than assuming the shape's presence
 * is what lets this code have a chance of running unchanged once it does,
 * but the mapping itself has not been checked against a real row.
 *
 * `applied_patch` is lifted out as `patch`; `model_proposal` is omitted
 * on purpose for this slice -- #353 says "what was suggested and what was
 * accepted are different facts, and a future run reads both", but this
 * interview's prompt only needs what was actually applied to steer away
 * from a repeat suggestion, not the model's original (possibly rejected)
 * proposal. A future run that wants both facts reads `model_proposal`
 * itself; this seam does not carry it. A `transcript` key some future row
 * carries is dropped here too, on the one path a past outcome has into
 * the prompt.
 *
 * @param {{grills?: Array<Record<string, unknown>>}} sweep
 * @param {string} itemId
 * @returns {Array<{summary: unknown, verdict: unknown, patch: unknown}>}
 */
export function pastOutcomes(sweep, itemId) {
  if (!Array.isArray(sweep.grills)) return [];
  return sweep.grills
    .filter((grill) => isObject(grill) && grill.item_id === itemId)
    .map((grill) => ({ summary: grill.summary, verdict: grill.verdict, patch: grill.applied_patch }));
}

export const grillMe = {
  name: "grill-me",

  /** Versioned beside the SKILL.md this ships with (#41 decision 4). */
  resultSchemaPath: ".claude/skills/grill-me/schema.json",

  /**
   * Shape only -- whether `ref` names a real item is `prepare`'s question,
   * answered in the authority's own terms, not a regex's. Each entry of
   * `turns` is checked against the schema's own `question` shape plus a
   * non-empty `answer`, so a malformed prior turn is a named 400 here,
   * before a model token is spent trying to make sense of it.
   *
   * @param {Record<string, unknown>} args
   * @returns {{ok: true} | {ok: false, error: string}}
   */
  validateArgs(args) {
    if (!isObject(args)) {
      return { ok: false, error: "args must be an object" };
    }
    if (!isNonEmptyString(args.ref)) {
      return { ok: false, error: '"ref" must be a non-empty item reference (HB-42 or a uuid)' };
    }
    if (!Array.isArray(args.turns)) {
      return {
        ok: false,
        error: '"turns" must be an array -- the conversation so far, empty to start the interview',
      };
    }
    for (const [index, turn] of args.turns.entries()) {
      if (!isTurnShape(turn)) {
        return {
          ok: false,
          error:
            `"turns[${index}]" is malformed -- each turn needs a question ` +
            "({prompt, recommendedAnswer, choices: 2-4 strings}) and a non-empty \"answer\"",
        };
      }
    }
    // Part of this op's stated args contract (#273's precedent), named
    // here even though `server.js` gates `model` for every skill.
    if (args.model !== undefined && !isValidModelId(args.model)) {
      return { ok: false, error: '"model" must be a model id when present' };
    }
    return { ok: true };
  },

  /**
   * The read half, run before the model -- the whole of this op's contact
   * with the authority. An unknown ref, a missing token and an unreachable
   * authority all end the stream here, in the authority's own words,
   * before a single model token is spent. Declining past the turn cap
   * happens here too, for the same reason: it costs nothing to check
   * `args.turns.length` against a constant.
   *
   * @param {Record<string, unknown>} args
   * @param {{authority: import("../authority.js").unconfiguredAuthority, onProgress: (message: string) => void}} deps
   */
  async prepare(args, { authority, onProgress }) {
    onProgress(`reading ${args.ref} from the authority`);

    const read = await authority.sweep();
    if (!read.ok) return { ok: false, error: `authority: ${read.error}` };

    // Unnamed by `authority:` on purpose -- the authority answered fine; it
    // is the ref that names nothing (the same posture `microtask` takes).
    const resolved = resolveRef(read.sweep, args.ref);
    if (!resolved.ok) return resolved;

    if (args.turns.length >= PROVISIONAL_TURN_CAP) {
      return {
        ok: false,
        error:
          `this interview has reached its ${PROVISIONAL_TURN_CAP}-turn cap for item ${resolved.item.id} -- ` +
          "the cap is provisional (#351 sets the real number); resolve it by hand or start a fresh grill",
      };
    }

    onProgress(`item ${resolved.item.id}: turn ${args.turns.length + 1}`);

    return {
      ok: true,
      args: {
        item: resolved.item,
        turns: args.turns,
        priorOutcomes: pastOutcomes(read.sweep, resolved.item.id),
      },
    };
  },

  /**
   * @param {Record<string, unknown>} args as returned by `prepare`
   * @returns {string}
   */
  buildPrompt(args) {
    return [
      "/grill-me",
      "",
      "Runner arm: the item has ALREADY been read from the authority and follows",
      "as JSON, along with this session's own turns so far and any PRIOR APPLIED",
      "grill outcomes for it (never a past transcript). You have no shell here --",
      "do not write anything, tick anything or call any other skill. Answer with",
      "exactly one typed turn against the schema: the next question, or, once the",
      "fog is settled or exhausted, the terminal proposal. Ask one thing at a",
      "time -- never a batch of questions in one turn.",
      "",
      JSON.stringify({ item: args.item, priorOutcomes: args.priorOutcomes, turns: args.turns }),
    ].join("\n");
  },
};
