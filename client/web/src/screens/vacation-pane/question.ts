import type { QuestionDef } from "../questions/contract";
import { CALENDAR_REQUEST_KEY, SUBJECT_KEY, vacationAnswer, vacationCalendarInterval } from "./vacation";
import { VacationPaneExpanded } from "./VacationPaneExpanded";

/** "How long to the next vacation?" as the shell's registry sees it (#121
 * over #245).
 *
 * One subject, always — the trip queue is one answer, not one pane per trip.
 * `sources: []`: this question touches no `context_snapshots` lane at all,
 * and raises no alerts either (see `vacation.ts`'s header for why that is a
 * decision rather than an omission). Its one read is #267's calendar arm,
 * declared below over the **long** horizon — the same −7d/+730d window
 * `CalendarHorizon::Long` polls, so the read never asks for an interval the
 * mirror was never filled for. */
export const vacationQuestion: QuestionDef = {
  label: "Next vacation",
  sources: [],
  subjects: () => [SUBJECT_KEY],
  answer: (_subjectKey, inputs) => vacationAnswer(inputs),
  calendarRequests: (nowMs) => {
    const interval = vacationCalendarInterval(nowMs);
    return [{ key: CALENDAR_REQUEST_KEY, ...interval }];
  },
  Expanded: VacationPaneExpanded,
};
