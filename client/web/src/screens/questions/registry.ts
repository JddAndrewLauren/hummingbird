import { githubQuestion } from "../github-pane/question";
import { kimiQuestion } from "../kimi-pane/question";
import { raceQuestion } from "../race-pane/question";
import { reachabilityQuestion } from "../reachability-pane/question";
import { uptimeQuestion } from "../uptime-pane/question";
import { wasteQuestion } from "../waste-pane/question";
import { vacationQuestion } from "../vacation-pane/question";
import { weekendQuestion } from "../weekend-pane/question";
import {
  QUESTION_ORDER,
  paneKey,
  type CalendarEventsRequest,
  type QuestionDef,
  type QuestionInputs,
  type RankedPane,
  type StandingQuestion,
  type Surface,
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
  weekend: weekendQuestion,
  vacation: vacationQuestion,
  race: raceQuestion,
  kimi: kimiQuestion,
  github: githubQuestion,
  uptime: uptimeQuestion,
  reachability: reachabilityQuestion,
};

/** `QUESTION_ORDER`, filtered to the questions declared for one surface
 * (ADR-0017 decision 1) — the one filter `rankPanes` and `requiredSources`
 * both apply, so the two can never disagree about which questions belong to
 * a view. */
function questionsFor(surface: Surface): StandingQuestion[] {
  return QUESTION_ORDER.filter((question) => QUESTIONS[question].surface === surface);
}

/** Every `context_snapshots` source one surface's registered questions must
 * request a pane read for — the union over that surface's questions,
 * deduplicated, in declared order. A question that reads no snapshot lane
 * (the calendar-lane ones, #117/#121/#122, and every never-polled infra
 * placeholder, #311) contributes nothing here and is not special-cased. */
export function requiredSources(surface: Surface): string[] {
  const sources: string[] = [];
  for (const question of questionsFor(surface)) {
    for (const source of QUESTIONS[question].sources) {
      if (!sources.includes(source)) {
        sources.push(source);
      }
    }
  }
  return sources;
}

/** Every calendar-arm interval the registered standing questions need —
 * `requiredSources`'s exact twin for the calendar lane, unioned over every
 * registered question's own `calendarRequests` in declared order (a
 * question that declares none, like `wasteQuestion`, contributes nothing).
 * Takes the clock because a declared interval can itself be a function of
 * it (#122's rolling weekend window) — `useCalendarEventsWiring.ts` is the
 * one caller, and passes its own `nowMs` straight through, which is what
 * stops the wiring and the registry drifting the way a caller-supplied
 * `requests` prop would let them. */
export function requiredCalendarRequests(nowMs: number): CalendarEventsRequest[] {
  const requests: CalendarEventsRequest[] = [];
  for (const question of QUESTION_ORDER) {
    const definition = QUESTIONS[question];
    if (definition.calendarRequests) {
      requests.push(...definition.calendarRequests(nowMs));
    }
  }
  return requests;
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

/** Every question declared for one surface, every subject, ranked (ADR-0015's
 * cross-pane order, ADR-0017's per-surface filter). A view never ranks a
 * question its surface cannot show — `NowScreen`'s aside asks for `"now"`,
 * the Status screen for `"status"`, and neither reads the other's panes.
 *
 * Pure and clock-free beyond the `nowMs` on `inputs` — which is what lets
 * `RankedRegion` capture one ranking in state and re-sample it on its own
 * terms, and what lets the demo fixture rank a hand-authored world through
 * the very same code the real region uses. */
export function rankPanes(inputs: QuestionInputs, surface: Surface): RankedPane[] {
  return panesFrom(QUESTIONS, questionsFor(surface), inputs);
}
