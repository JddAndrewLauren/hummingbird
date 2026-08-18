// What one NDJSON line from the skill runner *means* (#273).
//
// **The rules are `hummingbird_core::decisions::skills::envelope`'s** since
// #538 sank them there for the Android client (ADR-0025); this module is
// the call and the type declarations, and nothing else. Its exported
// signatures are unchanged, so `envelope.test.ts` still pins the rules from
// this side — the same shape every other sunk module in `src/decisions/`'s
// orbit takes.
//
// The wire contract is `docs/runner.md`'s: zero or more
// `{"type":"progress","message":…}` lines, then exactly one terminal
// `{ok, skill, result|error}` line, which past the dispatch also carries a
// `backend`/`model` stamp. The authority's proxy (ADR-0018) synthesizes its
// own failures in the same shape, unstamped, so there is exactly one line
// grammar to read and the client never branches on where a line came from.
//
// **`backend` and `model` are `string | null` and are never defaulted to a
// literal.** That is what makes #273's "rendered from the envelope, not
// hardcoded at the render site" true by construction rather than by review:
// there is no name in this file — nor in the core family behind it — for a
// render site to inherit.

import { classifySkillLine, grillResultFromCore, microtaskResultFromCore } from "../decisions/seam";

/** One classified line. `unreadable` is a line this client cannot parse —
 * dropped from the narration, never treated as terminal (a stream that
 * emits garbage mid-flight has not ended). */
export type SkillLine =
  | { kind: "progress"; message: string }
  | { kind: "ok"; result: unknown; backend: string | null; model: string | null }
  | { kind: "failed"; error: string; backend: string | null; model: string | null }
  | { kind: "unreadable" };

export function classifyLine(text: string): SkillLine {
  return classifySkillLine(text) as SkillLine;
}

/** What `microtask` answers against its schema. */
export interface MicrotaskResult {
  steps: string[];
  note: string;
}

/**
 * The terminal line's `result`, read as `microtask`'s own schema
 * (`.claude/skills/microtask/schema.json`). `null` when it is not that
 * shape — the run still succeeded, so this is not a failure; there is just
 * nothing to say about what came back beyond the stamp.
 *
 * The steps themselves are **not** read from here. They arrive through the
 * normal step read path, because the runner already wrote them to the
 * authority; taking them from this object would be the second source of
 * truth for steps that #273 forbids. `note` is what this is for.
 */
export function microtaskResult(result: unknown): MicrotaskResult | null {
  return microtaskResultFromCore(result) as MicrotaskResult | null;
}

/** ADR-0023's typed turn: `grill-me`'s own `question` shape
 * (`.claude/skills/grill-me/schema.json`) — 2-4 short `choices`, and free
 * text is always still a valid answer regardless of what they list. */
export interface GrillQuestion {
  prompt: string;
  recommendedAnswer: string;
  choices: string[];
}

/** `grill-me`'s terminal `proposal` shape. `verdict` is the wire's
 * snake_case spelling (`"resolved"`/`"fog_remains"`) — `hummingbird_domain`
 * `GrillVerdict`'s own vocabulary, not a second one. `patch` is whatever
 * item-field edits the interview turned up, as an opaque object: this
 * client never reads a key out of it itself, only carries it through to
 * `Core::complete_grill`'s `applied_patch`. */
export interface GrillProposal {
  summary: string;
  verdict: "resolved" | "fog_remains";
  patch: Record<string, unknown>;
}

export type GrillTurnResult =
  | { kind: "question"; question: GrillQuestion }
  | { kind: "proposal"; proposal: GrillProposal };

/**
 * The terminal line's `result`, read as `grill-me`'s own schema
 * (ADR-0023): exactly one of a `question` or a `proposal` turn, `null` for
 * anything else — including a result that names both or neither, which the
 * runner's own `oneOf` schema treats as a failed run in the first place, so
 * there is nothing more specific to say about it here than "not that
 * shape".
 */
export function grillResult(result: unknown): GrillTurnResult | null {
  return grillResultFromCore(result) as GrillTurnResult | null;
}
