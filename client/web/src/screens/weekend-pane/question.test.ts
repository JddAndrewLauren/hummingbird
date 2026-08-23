import { describe, expect, it } from "vitest";
import { weekendQuestion } from "./question";
import { dayKeyOf, weekendWindow } from "./weekend";

// The registered question's own declaration — specifically the calendar
// request's CIVIL bounds, which `weekend.test.ts` cannot see (it tests the
// window and the merge; this tests what is actually asked for).
//
// These two strings decide which all-day events the pane is ever handed
// (ADR-0015's 2026-08-10 amendment: an all-day event carries no instant, so
// the millisecond bounds cannot fetch it). An off-by-one on the exclusive
// end drops every Sunday all-day event and shows nothing at all — no error,
// no gap state, just a weekend that reads as free. Nothing else in the suite
// would catch it: `useCalendarEventsWiring.test.tsx` asserts these fields
// are present and date-shaped, never that they name the right days.

/** A Wednesday, so the window is the *coming* weekend rather than one under
 * way — the ordinary case, and the one where the fetched range and the
 * rendered days are computed furthest apart in time. */
const WEDNESDAY = new Date(2026, 7, 12, 10, 0).getTime();

function request(nowMs: number) {
  const requests = weekendQuestion.calendarRequests?.(nowMs) ?? [];
  expect(requests).toHaveLength(1);
  return requests[0];
}

describe("weekendQuestion.calendarRequests", () => {
  it("asks from Friday's own day key, not from the window's 17:00 start", () => {
    // `window.startMs` is Friday 17:00 — the band's "has the weekend
    // started" instant — but an all-day event on Friday is a fact about the
    // whole day. Deriving the civil start from `startMs` would still land
    // on Friday here, so this pins the intent rather than the arithmetic:
    // it must be the day key the merge itself buckets by.
    const window = weekendWindow(WEDNESDAY);
    expect(request(WEDNESDAY).startDate).toBe(window.days[0].key);
  });

  it("asks up to an EXCLUSIVE end of the day after Sunday", () => {
    const window = weekendWindow(WEDNESDAY);
    const sunday = window.days[2].key;
    const { endDate } = request(WEDNESDAY);

    // Strictly after Sunday: an `endDate` of Sunday itself would exclude
    // Sunday, since the whole comparison is half-open on both sides.
    expect(endDate > sunday).toBe(true);
    expect(endDate).toBe(dayKeyOf(window.endMs + 1));
  });

  it("covers every day the merge renders — the fetch and the render agree", () => {
    // The property that actually matters, stated over the three days
    // rather than over the endpoints: an all-day event on any rendered day
    // must fall inside `[startDate, endDate)`, or the pane asks for a
    // window it then claims to have answered.
    const window = weekendWindow(WEDNESDAY);
    const { startDate, endDate } = request(WEDNESDAY);

    for (const day of window.days) {
      expect(startDate <= day.key && day.key < endDate).toBe(true);
    }
  });

  it("shrinks its civil start with the window, and keeps both arms on the same day", () => {
    // Sunday 09:00: the window has shrunk to Sunday alone, so asking from
    // Friday would fetch two days of events no column can hold — and the
    // timed arm has to name the same opening day as the civil one, or a
    // Sunday all-day event arrives against a Friday-17:00 lower bound.
    const sundayMorning = new Date(2026, 7, 16, 9, 0).getTime();
    const window = weekendWindow(sundayMorning);
    expect(window.days.map((day) => day.key)).toHaveLength(1);

    const { startDate, endDate, startMs } = request(sundayMorning);
    expect(startDate).toBe(window.days[0].key);
    expect(startMs).toBe(window.days[0].startMs);
    expect(dayKeyOf(startMs)).toBe(startDate);
    for (const day of window.days) {
      expect(startDate <= day.key && day.key < endDate).toBe(true);
    }
  });

  it("leaves the timed start at Friday 17:00 while the whole weekend is ahead", () => {
    // The raise only bites once days start dropping: before the weekend,
    // Friday midnight is below Friday 17:00, so `startMs` is the window's
    // own start exactly as it always was.
    expect(request(WEDNESDAY).startMs).toBe(weekendWindow(WEDNESDAY).startMs);
  });

  it("rolls its civil bounds forward with the window itself", () => {
    // Sunday 21:00: `weekendWindow` has already rolled to next weekend, so
    // the request must have too — a civil arm computed from a stale window
    // would fetch last weekend and silently render nothing.
    const sundayLate = new Date(2026, 7, 16, 21, 0).getTime();
    const rolled = weekendWindow(sundayLate);

    expect(request(sundayLate).startDate).toBe(rolled.days[0].key);
    expect(request(sundayLate).startDate > request(WEDNESDAY).startDate).toBe(true);
  });
});
