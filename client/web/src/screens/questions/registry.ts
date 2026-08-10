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

/** Every question, every subject, ranked (ADR-0015's cross-pane order).
 *
 * Pure and clock-free beyond the `nowMs` on `inputs` — which is what lets
 * `RankedRegion` capture one ranking in state and re-sample it on its own
 * terms, and what lets the demo fixture rank a hand-authored world through
 * the very same code the real region uses. */
export function rankPanes(inputs: QuestionInputs): RankedPane[] {
  const panes: RankedPane[] = [];
  for (const question of QUESTION_ORDER) {
    const definition = QUESTIONS[question];
    for (const subjectKey of definition.subjects(inputs)) {
      panes.push({
        question,
        subjectKey,
        paneKey: paneKey(question, subjectKey),
        answer: definition.answer(subjectKey, inputs),
      });
    }
  }
  return orderPanes(panes, QUESTION_ORDER);
}
