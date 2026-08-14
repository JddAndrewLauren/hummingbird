// The state of one Grill turn request (#355, ADR-0023) — `run-state.ts`'s
// `reduceRun`/`SkillRunState` for `microtask`, adapted for `grill-me`'s own
// terminal shape: an `"ok"` line answers with either the next `question` or
// the terminal `proposal` (`envelope.ts`'s `grillResult`), never a bare
// `note`, so the "done" phase there has no equivalent here — this reducer
// grows a `"question"` and a `"proposal"` phase in its place.
//
// Same four invariants `run-state.ts` documents for its own reducer: "in
// flight", "declined" and each of the two answered shapes are
// distinguishable phases, and the duplicate-tap rule (a `"started"` while
// already `"asking"` is a no-op) lives here rather than in a click handler.

import { grillResult, type GrillProposal, type GrillQuestion } from "./envelope";
import type { SkillEvent } from "./run-state";

/** A run that answered `ok:true` with a result outside `grill-me`'s own
 * `oneOf` schema. The runner's schema validation is meant to make this
 * unreachable in practice (`.claude/skills/grill-me/SKILL.md`: "Your only
 * failure mode is answering outside the schema") — this is the client's
 * own backstop for that promise not holding, not a case this reducer
 * expects to exercise against a well-behaved runner. */
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
  switch (event.kind) {
    case "started":
      return state.phase === "asking" ? state : { phase: "asking", messages: [] };

    case "progress":
      if (state.phase !== "asking") return state;
      return { phase: "asking", messages: append(state.messages, event.message) };

    case "ok": {
      if (state.phase !== "asking") return state;
      const result = grillResult(event.result);
      if (result === null) {
        return declined(state.messages, OUTSIDE_SCHEMA, event.backend, event.model, true);
      }
      return result.kind === "question"
        ? { phase: "question", messages: state.messages, question: result.question, backend: event.backend, model: event.model }
        : { phase: "proposal", messages: state.messages, proposal: result.proposal, backend: event.backend, model: event.model };
    }

    case "failed":
      if (state.phase !== "asking") return state;
      return declined(
        state.messages,
        event.error,
        event.backend,
        event.model,
        "answered" in event ? event.answered : false,
      );

    case "unreadable":
      return state;
  }
}

function declined(
  messages: string[],
  reason: string,
  backend: string | null,
  model: string | null,
  answered: boolean,
): GrillTurnState {
  return { phase: "declined", messages, reason, backend, model, answered };
}

/** The runner heartbeats `"still running"` every 20s — same de-duplication
 * `run-state.ts`'s own `append` applies. */
function append(messages: string[], message: string): string[] {
  return messages[messages.length - 1] === message ? messages : [...messages, message];
}
