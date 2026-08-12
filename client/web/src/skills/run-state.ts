// The state of one skill run, as something a reader can observe (#273).
//
// The acceptance criterion is that "in flight", "declined with reason" and
// "completed" are distinguishable — so they are four named phases rather
// than a bag of booleans, and every phase has exactly one rendering.
//
// **The duplicate-tap rule lives here**, in `reduceRun`: a `started` while a
// run is already streaming leaves the state untouched. Putting it in the
// reducer rather than in the click handler is what makes it testable
// without a DOM, and it is why the button's `disabled` is a second
// expression of the rule rather than the only one.

import type { SkillLine } from "./envelope";
import { microtaskResult } from "./envelope";
import { NO_TERMINAL_LINE } from "./decline";

export type SkillEvent = { kind: "started" } | SkillLine;

export type SkillRunState =
  | { phase: "idle" }
  | { phase: "running"; messages: string[] }
  | {
      phase: "done";
      messages: string[];
      /** `microtask`'s own `note` — #307 point 7 puts what-was-kept there
       * on purpose. Empty when the result did not carry one. */
      note: string;
      backend: string | null;
      model: string | null;
    }
  | {
      phase: "declined";
      messages: string[];
      /** Verbatim: the seam's own words, or the transport's. Never
       * prefixed, never branched on. */
      reason: string;
      backend: string | null;
      model: string | null;
    };

export const IDLE: SkillRunState = { phase: "idle" };

export function isRunning(state: SkillRunState): boolean {
  return state.phase === "running";
}

/**
 * The stamp as one line of text, or `null` when there is nothing honest to
 * say. Absent when the envelope named no backend — an unstamped line means
 * nothing was attempted (ADR-0018), and a stamp invented at the render site
 * is exactly what #273 forbids.
 */
export function stampLabel(state: SkillRunState): string | null {
  if (state.phase !== "done" && state.phase !== "declined") return null;
  if (state.backend === null) return null;
  return state.model === null ? state.backend : `${state.backend} · ${state.model}`;
}

export function reduceRun(state: SkillRunState, event: SkillEvent): SkillRunState {
  switch (event.kind) {
    case "started":
      // The duplicate-tap rule. A second tap during a streaming run must not
      // wipe the narration already on screen, and must not start anything.
      return state.phase === "running" ? state : { phase: "running", messages: [] };

    case "progress":
      // Only a running state accumulates. A line arriving after the terminal
      // one (or before a tap, which cannot happen but costs nothing to
      // exclude) is dropped rather than reopening a finished run.
      if (state.phase !== "running") return state;
      return { phase: "running", messages: append(state.messages, event.message) };

    case "ok": {
      if (state.phase !== "running") return state;
      const result = microtaskResult(event.result);
      return {
        phase: "done",
        messages: state.messages,
        note: result?.note ?? "",
        backend: event.backend,
        model: event.model,
      };
    }

    case "failed":
      if (state.phase !== "running") return state;
      return {
        phase: "declined",
        messages: state.messages,
        reason: event.error,
        backend: event.backend,
        model: event.model,
      };

    case "unreadable":
      // Dropped from the narration, and deliberately NOT terminal: a stream
      // that emits one garbage line has not ended. If the stream then ends
      // with nothing terminal at all, the seam synthesizes the terminal
      // event carrying `NO_TERMINAL_LINE` — this reducer never has to guess
      // that a run is over.
      return state;
  }
}

/** The runner heartbeats `"still running"` every 20s, and an op can repeat
 * a line of its own; a narration that stutters the same sentence adds
 * nothing a reader can use. */
function append(messages: string[], message: string): string[] {
  return messages[messages.length - 1] === message ? messages : [...messages, message];
}

export { NO_TERMINAL_LINE };
