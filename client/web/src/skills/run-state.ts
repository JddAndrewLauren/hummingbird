// The state of one skill run, as something a reader can observe (#273).
//
// **The rules are `hummingbird_core::decisions::skills::run`'s** since #538
// sank them there for the Android client (ADR-0025); this module is the
// call, the type declarations and nothing else, and `run-state.test.ts`
// still pins them from this side.
//
// The acceptance criterion is that "in flight", "declined with reason" and
// "completed" are distinguishable — so they are four named phases rather
// than a bag of booleans, and every phase has exactly one rendering.
//
// **The duplicate-tap rule lives in the reducer**, not in the click
// handler: a `started` while a run is already streaming leaves the state
// untouched — and untouched *by identity*, because the seam hands back the
// object it was given when the core's answer is byte-identical (see
// `decisions/seam.ts`'s M4 header). That is what makes the rule testable
// without a DOM, and it is why the button's `disabled` is a second
// expression of the rule rather than the only one.

import { reduceSkillRun, skillStampLabel } from "../decisions/seam";
import type { SkillLine } from "./envelope";
import { NO_TERMINAL_LINE } from "./decline";

/**
 * A terminal `failed` that **routing** has annotated with the one fact no
 * line carries: whether a backend actually answered this run (#274).
 *
 * It is not a property of a line — the wire never sends it, and
 * `classifyLine` never sets it. Only `route-run.ts` knows it, from whether
 * its `fetch` resolved, and the distinction is not recoverable downstream:
 * a seam decline (#307), a 401 and a rejected connection can all arrive as
 * an unstamped `failed`, and `decline.ts` forbids telling them apart by
 * their prose.
 */
export interface RoutedFailure {
  kind: "failed";
  error: string;
  backend: string | null;
  model: string | null;
  answered: boolean;
}

export type SkillEvent = { kind: "started" } | SkillLine | RoutedFailure;

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
      /** Whether a backend answered this run (#274). A decline that a
       * backend *answered* is not evidence any backend is unreachable, so
       * nothing may offer switching away from one on the strength of it.
       * `false` whenever the event did not say — an unrouted run, or a
       * caller feeding this reducer raw envelope lines. */
      answered: boolean;
    };

export const IDLE: SkillRunState = { phase: "idle" };

/** A phase read, not a rule: the state names its own phase, and there is
 * nothing for two clients to disagree about. */
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
  return skillStampLabel(state);
}

export function reduceRun(state: SkillRunState, event: SkillEvent): SkillRunState {
  return reduceSkillRun(state, event);
}

export { NO_TERMINAL_LINE };
