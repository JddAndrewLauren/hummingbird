// The `POST /api/skills/run` body for one Grill turn (#355, ADR-0023).
// `grill-me` is stateless — every request carries the whole conversation
// so far (`.claude/skills/grill-me/SKILL.md`'s "every request carries the
// whole conversation") — so this module's whole job is threading `turns`
// back exactly as the runner requires them, never trimming or replaying a
// subset of what actually happened.

import type { GrillQuestion } from "./envelope";

/** One completed round: the question the model asked, and the answer given
 * to it — free text always, never constrained to `question.choices` (the
 * runner's own rule, restated in `grill-me`'s SKILL.md: "the answer is
 * always free text too"). */
export interface GrillTurn {
  question: GrillQuestion;
  answer: string;
}

export interface GrillRunInput {
  /** The item's uuid, or `HB-<seq>` — same `ref` vocabulary
   * `microtask-args.ts` documents for its own field, though this caller
   * always has a uuid to hand (an item already open in Triage). */
  ref: string;
  turns: GrillTurn[];
  /** Omitted leaves the runner's configured model — same rule
   * `microtask-args.ts` applies. */
  model?: string;
}

export interface GrillRunBody {
  skill: "grill-me";
  args: Record<string, unknown>;
}

/** `model` is omitted when unset, so the runner's own default stays the
 * default rather than being re-specified here — the identical rule
 * `microtaskRunBody` applies for the same field. */
export function grillRunBody(input: GrillRunInput): GrillRunBody {
  const args: Record<string, unknown> = { ref: input.ref, turns: input.turns };
  if (input.model !== undefined && input.model !== "") args.model = input.model;
  return { skill: "grill-me", args };
}

/** The plain-text record `Core::complete_grill`'s `GrillCompletion.transcript`
 * carries (ADR-0023 decision 2: "the transcript that produced it"). Opaque
 * on this client's own read path — nothing here ever parses it back — so a
 * readable Q/A listing is exactly as good as any other encoding, and far
 * easier for a human to read back on the review card or a future history
 * surface. `[]` (no turns yet — the review card can only ever render past a
 * proposal, so this is defensive rather than reachable in practice) reads
 * as the empty string, never a placeholder sentence. */
export function formatGrillTranscript(turns: GrillTurn[]): string {
  return turns.map((turn) => `Q: ${turn.question.prompt}\nA: ${turn.answer}`).join("\n\n");
}
