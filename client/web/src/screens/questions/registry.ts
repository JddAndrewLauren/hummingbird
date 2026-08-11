import { wasteQuestion } from "../waste-pane/question";
import {
  QUESTION_ORDER,
  paneKey,
  type QuestionDef,
  type QuestionInputs,
  type RankedPane,
  type StandingQuestion,
} from "./contract";
import { orderPanes } from "./sort";

// The registry: every standing question this build renders, and the one
// function that turns the inputs into a ranked list of panes.
//
// `Record<StandingQuestion, QuestionDef>` rather than an array, deliberately:
// the type is exhaustive, so adding a question to the vocabulary without
// registering it is a compile error rather than a pane that silently never
// appears — the same compile-time-exhaustive shape `request-router.ts`'s
// `TASK_REQUEST_TYPES` uses, and for the same reason.

export const QUESTIONS: Record<StandingQuestion, QuestionDef> = {
  waste: wasteQuestion,
};

/** Every `context_snapshots` source the wiring must request a pane read for
 * — the union over every registered question, deduplicated, in declared
 * order. A question that reads no snapshot lane (the calendar-lane ones,
 * #117/#121/#122) contributes nothing here and is not special-cased. */
export function requiredSources(): string[] {
  const sources: string[] = [];
  for (const question of QUESTION_ORDER) {
    for (const source of QUESTIONS[question].sources) {
      if (!sources.includes(source)) {
        sources.push(source);
      }
    }
  }
  return sources;
}

/** One `getCalendarEvents` request `useCalendarEventsWiring.ts` posts —
 * defined here, not in that hook, for the same reason `requiredSources`'s
 * return type lives here: the registry is the one place a question's
 * requirements are declared, and the wiring only ever reads them. */
export interface CalendarEventsRequest {
  /** The caller's own identity for this request — becomes the
   * `QuestionInputs.calendarReads` map key. */
  key: string;
  startMs: number;
  endMs: number;
}

/** Every calendar-arm interval the registered standing questions need —
 * `requiredSources`'s exact twin for the calendar lane, unioned over every
 * registered question in declared order. No shipped `QuestionDef` declares
 * one yet (#122's job — the calendar-lane questions, #117/#121/#122, are
 * still prototypes), so this returns empty today; a question that starts
 * asking for one is what `useCalendarEventsWiring.ts` must pick up without
 * itself changing, which is what stops the wiring and the registry
 * drifting the way a caller-supplied `requests` prop would let them. */
export function requiredCalendarRequests(): CalendarEventsRequest[] {
  return [];
}

/** The 0..N expansion itself: every question in `order`, every subject it
 * currently has, each with its answer — ranked.
 *
 * Takes its registry rather than reading the module's, for one reason: no
 * shipped question emits more than one subject (or none), so the only way
 * the expansion is exercised at all is a test registry running through this
 * exact code. A test that hand-built panes and called `orderPanes` would
 * leave the loop below — the thing the 0..N contract is about — untested. */
export function panesFrom(
  questions: Record<StandingQuestion, QuestionDef>,
  order: readonly StandingQuestion[],
  inputs: QuestionInputs,
): RankedPane[] {
  const panes: RankedPane[] = [];
  for (const question of order) {
    const definition = questions[question];
    for (const subjectKey of definition.subjects(inputs)) {
      panes.push({
        question,
        subjectKey,
        paneKey: paneKey(question, subjectKey),
        answer: definition.answer(subjectKey, inputs),
      });
    }
  }
  return orderPanes(panes, order);
}

/** Every question, every subject, ranked (ADR-0015's cross-pane order).
 *
 * Pure and clock-free beyond the `nowMs` on `inputs` — which is what lets
 * `RankedRegion` capture one ranking in state and re-sample it on its own
 * terms, and what lets the demo fixture rank a hand-authored world through
 * the very same code the real region uses. */
export function rankPanes(inputs: QuestionInputs): RankedPane[] {
  return panesFrom(QUESTIONS, QUESTION_ORDER, inputs);
}
