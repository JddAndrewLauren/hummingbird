// The state of one Grill turn request (#355, ADR-0023) — `run-state.ts`'s
// `reduceRun`/`SkillRunState` for `microtask`, adapted for `grill-me`'s own
// terminal shape: an `"ok"` line answers with either the next `question` or
// the terminal `proposal` (`envelope.ts`'s `grillResult`), never a bare
// `note`, so the "done" phase there has no equivalent here — this reducer
// grows a `"question"` and a `"proposal"` phase in its place.
//
// **The rules are `hummingbird_core::decisions::skills::grill`'s** since
// #538 sank them there (ADR-0025); this module is the call and the types.
// Same four invariants `run-state.ts` documents for its own reducer: "in
// flight", "declined" and each of the two answered shapes are
// distinguishable phases, and the duplicate-tap rule (a `"started"` while
// already `"asking"` is a no-op) lives in the reducer rather than in a
// click handler.

import { reduceGrillTurn as reduceGrillTurnThroughCore } from "../decisions/seam";
import type { GrillProposal, GrillQuestion } from "./envelope";
import type { SkillEvent } from "./run-state";

/** A run that answered `ok:true` with a result outside `grill-me`'s own
 * `oneOf` schema. The runner's schema validation is meant to make this
 * unreachable in practice (`.claude/skills/grill-me/SKILL.md`: "Your only
 * failure mode is answering outside the schema") — this is the client's
 * own backstop for that promise not holding, not a case this reducer
 * expects to exercise against a well-behaved runner.
 *
 * **A literal, pinned against the core rather than read from it.** This is
 * a module-evaluation-time `const` on a path statically reachable from
 * `main.tsx`, where a seam call would throw the "used before ready" guard
 * on every page load — the same constraint `priority.ts`'s `priorityRank`
 * and `field-vocabulary.ts`'s arrays live under. `seam.test.ts` pins it
 * equal to `hummingbird_core::decisions::skills::grill::OUTSIDE_SCHEMA`, so
 * the copy cannot drift silently; ADR-0025's #538 amendment records the
 * carve-out as a verdict-table row. */
export const OUTSIDE_SCHEMA = "The run answered outside the schema.";

export type GrillTurnState =
  | { phase: "idle" }
  | { phase: "asking"; messages: string[] }
  | {
      phase: "question";
      messages: string[];
      question: GrillQuestion;
      backend: string | null;
      model: string | null;
    }
  | {
      phase: "proposal";
      messages: string[];
      proposal: GrillProposal;
      backend: string | null;
      model: string | null;
    }
  | {
      phase: "declined";
      messages: string[];
      /** Verbatim: the seam's own words, or the transport's. Never
       * prefixed, never branched on — same rule `run-state.ts` states for
       * its own `reason`. */
      reason: string;
      backend: string | null;
      model: string | null;
      answered: boolean;
    };

export const IDLE: GrillTurnState = { phase: "idle" };

export function reduceGrillTurn(state: GrillTurnState, event: SkillEvent): GrillTurnState {
  return reduceGrillTurnThroughCore(state, event);
}
