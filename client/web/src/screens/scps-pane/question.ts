import type { QuestionDef } from "../questions/contract";
import { CALENDAR_REQUEST_KEY, SUBJECT_KEY, scpsAnswer, scpsCalendarInterval } from "./scps";
import { ScpsPaneExpanded } from "./ScpsPaneExpanded";

/** "When is the next SCPS event, and what is this month's Photo Quest" —
 * the tenth standing question (#693, ADR-0032), the shell's own registry
 * entry.
 *
 * One subject, always — the event queue is one answer, not one pane per
 * event. `sources: []`: this question touches no `context_snapshots` lane
 * at all (ADR-0032 rejects the poller shape every other externally-fed
 * question uses; the agent writes straight into the calendar and a
 * binding). Its one read is #267's calendar arm, over the **standard**
 * horizon — 6 hours behind now, 90 days ahead, the same window
 * `CalendarHorizon::Standard` polls. */
export const scpsQuestion: QuestionDef = {
  label: "Next SCPS event",
  surface: "now",
  sources: [],
  subjects: () => [SUBJECT_KEY],
  answer: (_subjectKey, inputs) => scpsAnswer(inputs),
  calendarRequests: (nowMs) => {
    const interval = scpsCalendarInterval(nowMs);
    return [{ key: CALENDAR_REQUEST_KEY, ...interval }];
  },
  Expanded: ScpsPaneExpanded,
};
