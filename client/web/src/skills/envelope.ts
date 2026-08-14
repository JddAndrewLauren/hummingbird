// What one NDJSON line from the skill runner *means* (#273).
//
// The wire contract is `docs/runner.md`'s: zero or more
// `{"type":"progress","message":…}` lines, then exactly one terminal
// `{ok, skill, result|error}` line, which past the dispatch also carries a
// `backend`/`model` stamp. The authority's proxy (ADR-0018) synthesizes its
// own failures in the same shape, unstamped, so there is exactly one line
// grammar to read and the client never branches on where a line came from.
//
// **`backend` and `model` are `string | null` and are never defaulted to a
// literal here.** That is what makes #273's "rendered from the envelope,
// not hardcoded at the render site" true by construction rather than by
// review: there is no name in this file for a render site to inherit.

/** One classified line. `unreadable` is a line this client cannot parse —
 * dropped from the narration, never treated as terminal (a stream that
 * emits garbage mid-flight has not ended). */
export type SkillLine =
  | { kind: "progress"; message: string }
  | { kind: "ok"; result: unknown; backend: string | null; model: string | null }
  | { kind: "failed"; error: string; backend: string | null; model: string | null }
  | { kind: "unreadable" };

const UNREADABLE: SkillLine = { kind: "unreadable" };

export function classifyLine(text: string): SkillLine {
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    return UNREADABLE;
  }
  if (typeof value !== "object" || value === null || Array.isArray(value)) return UNREADABLE;
  const line = value as Record<string, unknown>;

  if (line.type === "progress") {
    return typeof line.message === "string" ? { kind: "progress", message: line.message } : UNREADABLE;
  }

  // `ok` must be a real boolean. A string `"true"` is a malformed line, not
  // a success — truthy-coercing it would render a failed run as a finished
  // one.
  if (line.ok === true) {
    return { kind: "ok", result: line.result, ...stamp(line) };
  }
  if (line.ok === false) {
    return typeof line.error === "string"
      ? { kind: "failed", error: line.error, ...stamp(line) }
      : UNREADABLE;
  }
  return UNREADABLE;
}

function stamp(line: Record<string, unknown>): { backend: string | null; model: string | null } {
  return {
    backend: typeof line.backend === "string" ? line.backend : null,
    model: typeof line.model === "string" ? line.model : null,
  };
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

function isGrillQuestion(value: unknown): value is GrillQuestion {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const question = value as Record<string, unknown>;
  return (
    typeof question.prompt === "string" &&
    typeof question.recommendedAnswer === "string" &&
    Array.isArray(question.choices) &&
    question.choices.length >= 2 &&
    question.choices.length <= 4 &&
    question.choices.every((choice) => typeof choice === "string")
  );
}

function isGrillProposal(value: unknown): value is GrillProposal {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const proposal = value as Record<string, unknown>;
  return (
    typeof proposal.summary === "string" &&
    (proposal.verdict === "resolved" || proposal.verdict === "fog_remains") &&
    typeof proposal.patch === "object" &&
    proposal.patch !== null &&
    !Array.isArray(proposal.patch)
  );
}

/**
 * The terminal line's `result`, read as `grill-me`'s own schema
 * (ADR-0023): exactly one of a `question` or a `proposal` turn, `null` for
 * anything else — including a result that names both or neither, which the
 * runner's own `oneOf` schema treats as a failed run in the first place, so
 * there is nothing more specific to say about it here than "not that
 * shape".
 */
export function grillResult(result: unknown): GrillTurnResult | null {
  if (typeof result !== "object" || result === null || Array.isArray(result)) return null;
  const value = result as Record<string, unknown>;
  if (value.kind === "question" && isGrillQuestion(value.question)) {
    return { kind: "question", question: value.question };
  }
  if (value.kind === "proposal" && isGrillProposal(value.proposal)) {
    return { kind: "proposal", proposal: value.proposal };
  }
  return null;
}

export function microtaskResult(result: unknown): MicrotaskResult | null {
  if (typeof result !== "object" || result === null || Array.isArray(result)) return null;
  const value = result as Record<string, unknown>;
  const steps = Array.isArray(value.steps)
    ? value.steps.filter((step): step is string => typeof step === "string")
    : [];
  const note = typeof value.note === "string" ? value.note : "";
  if (steps.length === 0 && note === "") return null;
  return { steps, note };
}
