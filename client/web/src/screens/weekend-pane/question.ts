import type { QuestionDef } from "../questions/contract";
import {
  CALENDAR_REQUEST_KEY,
  dayKeyOf,
  SUBJECT_KEY,
  weekendAnswer,
  weekendWindow,
} from "./weekend";
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
    return [
      {
        key: CALENDAR_REQUEST_KEY,
        startMs: window.startMs,
        endMs: window.endMs,
        // The same window in civil dates, resolved here in the device's own
        // zone — the arm all-day events are asked about (ADR-0015's
        // 2026-08-10 amendment; the core owns no tzdb and derives neither
        // half from the other). The lower bound is Friday's own day key
        // rather than `startMs`'s (Friday 17:00), for exactly the reason
        // `inWindow` documents: an all-day event covering Friday is a fact
        // about the whole day, and a window starting at 17:00 would ask
        // about Saturday onwards. The upper bound is EXCLUSIVE, so it is
        // the day after Sunday — `endMs` is 23:59:59.999, and one
        // millisecond later is Monday local midnight.
        //
        // `days[0]` is guarded the same way `inWindow` guards it: today
        // `weekendWindow` always returns exactly three days, so the
        // fallback is unreachable — but this runs inside
        // `useCalendarEventsWiring`'s effect, where a throw takes the whole
        // hook down rather than costing one request.
        startDate: window.days[0]?.key ?? dayKeyOf(window.startMs),
        endDate: dayKeyOf(window.endMs + 1),
      },
    ];
  },
  Expanded: WeekendPaneExpanded,
};
