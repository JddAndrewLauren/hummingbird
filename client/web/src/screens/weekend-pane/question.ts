import type { QuestionDef } from "../questions/contract";
import { CALENDAR_REQUEST_KEY, SUBJECT_KEY, weekendAnswer, weekendWindow } from "./weekend";
import { WeekendPaneExpanded } from "./WeekendPaneExpanded";

/** "What are my plans this weekend?" as the shell's registry sees it
 * (#122 over #245).
 *
 * One subject, always — the coming (or under-way) weekend is the only thing
 * this question ever answers. `sources: []`: this question touches no
 * `context_snapshots` lane at all — its one read is #267's calendar arm,
 * declared below via `calendarRequests`, plus `QuestionInputs.items`, which
 * every question can already read. It is returned even while the calendar
 * is unbound, because the setup prompt is how anyone discovers the question
 * exists — the same "still returns one sentinel subject" contract
 * `wasteQuestion` documents for its own binding. */
export const weekendQuestion: QuestionDef = {
  label: "This weekend",
  sources: [],
  subjects: () => [SUBJECT_KEY],
  answer: (_subjectKey, inputs) => weekendAnswer(inputs),
  calendarRequests: (nowMs) => {
    const window = weekendWindow(nowMs);
    return [{ key: CALENDAR_REQUEST_KEY, startMs: window.startMs, endMs: window.endMs }];
  },
  Expanded: WeekendPaneExpanded,
};
