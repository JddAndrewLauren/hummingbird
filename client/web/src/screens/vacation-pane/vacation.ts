import type { CalendarEventDTO, CalendarReadDTO, FreshnessDTO } from "../../store/protocol";
import { tripsCalendarId } from "../../calendar/selection";
import type { Band, PaneAnswer, QuestionInputs } from "../questions/contract";
import {
  addCivilDays,
  civilDateInZone,
  civilDaysBetween,
  deviceCivilToday,
  type CivilDate,
} from "../waste-pane/zoned-day";

// **How long to the next vacation** (#121, ADR-0015), rewritten from
// `screens/prototype-vacation-pane/` (deleted with this slice — variant A won
// on 2026-08-10; the settled UI verdicts are not re-litigated here).
//
// Three things about this module are load-bearing, and each was got wrong
// somewhere before it got here:
//
// **Civil dates only, never a subtraction of instants.** A trip is a range of
// *days* at a place, not a span of milliseconds. Every count below is
// `civilDaysBetween` over two `YYYY-MM-DD` values, the trip's dates resolve in
// **the event's own carried `EventTime.timeZone`**, and "today" resolves in
// the **device's** zone (`zoned-day.ts`). `endMs - DAY` appears nowhere: an
// all-day event's end is the provider's exclusive end — local midnight
// *after* the last day — so the last day is that civil date minus one **day
// on the calendar**. Getting this wrong is the "India in 394 days" defect
// ADR-0015 records here, and the same arithmetic corrupts `returns_today`.
//
// **Any booked trip keeps the pane out of `dormant`, however far away.**
// `collapse.ts` collapses a dormant pane by default, and this pane sits
// quietly for 380 of a trip's 395 days and is still worth reading. Dormant
// here means *there is nothing to count to*.
//
// **This question raises no alerts, by construction** — deliberately, unlike
// every sibling pane in #117. There is no material change to report: the
// number goes down by one each day, on cadence. `subject_key` is unused, and
// `liveAlerts` is never read.

/** The one subject this question ever has — present even while unbound, so
 * the setup prompt is discoverable (`waste.ts`'s own reasoning). */
export const SUBJECT_KEY = "next-trip";

/** The `QuestionInputs.calendarReads` key this pane requests under (#267). */
export const CALENDAR_REQUEST_KEY = "vacation";

const DAY_MS = 24 * 60 * 60 * 1000;

/** The interval this question needs from the calendar mirror, in days —
 * `hummingbird_core::calendar::CalendarHorizon::Long`'s own window, so the
 * read never asks for more than the poller was ever told to fetch. */
export const HORIZON_BEFORE_DAYS = 7;
export const HORIZON_AHEAD_DAYS = 730;

/** How the empty answer names its own horizon. The pane cannot tell
 * "genuinely nothing booked" from "booked beyond what this device polls", so
 * it must not claim the former — ADR-0015 makes nothing-in-horizon
 * `answered`, and a bare "Nothing booked" would make that answer a lie. */
export const HORIZON_LABEL = "2 years";

/** Beyond this the calendar read is stale. Declared **beside the band
 * function** rather than on the Rust `Freshness` type, because the driver is
 * the cost of a wrong answer here and nowhere else (`waste.ts`'s `26h`
 * precedent). 24h: the calendar polls every 15 minutes, so a whole day of
 * silence is a real fault — but unlike the waste pane, staleness never
 * suppresses the answer. A trip 45 days out does not rot, and withholding a
 * correct countdown to report a poller problem is inverted priorities. */
export const STALE_AFTER_MS = 24 * 60 * 60 * 1000;

/** Bands: within a week, within a month, and everything further out. */
const IMMINENT_WITHIN_DAYS = 7;
const NEAR_WITHIN_DAYS = 30;

/** Where a trip sits relative to today. Five, not three: the day you leave
 * and the day you come home are each their own sentence, and `returns_today`
 * is what the issue's "the day you land home it is already counting to the
 * next one" was loosely reaching for — the trip is still live until the
 * provider's exclusive end. */
export type TripPhase = "upcoming" | "departs_today" | "under_way" | "returns_today" | "past";

export interface Trip {
  id: string;
  /** The title with a leading `Trip:`/`Holiday:` removed — and nothing else
   * rewritten. */
  name: string;
  location: string | null;
  /** First day, in the event's own zone. */
  startDate: CivilDate;
  /** Last day — the exclusive end's civil date minus one **civil day**. */
  lastDate: CivilDate;
  /** The event's own boundaries, carried through for `withinBand` alone
   * (an instant is what the shell sorts on) — never for a day count. */
  startMs: number;
  endMs: number;
  phase: TripPhase;
  /** Whole civil days from today to the first day; 0 on the departure day,
   * negative once it has started. */
  daysUntil: number;
  /** Total civil days the trip covers, both ends included. */
  lengthDays: number;
  /** Which day of the trip today is, 1-based; 0 while it is still upcoming. */
  dayOfTrip: number;
}

/** "Trip: India" and "Holiday — India" both read as "India" in a countdown
 * sentence. Anything else is left exactly as it was typed: the calendar is
 * the authority (#117), and rewriting its titles would be the pane keeping a
 * vacation record of its own. */
export function tripName(title: string): string {
  return title.replace(/^\s*(trip|holiday|vacation|hols)\s*[:—-]\s*/i, "").trim() || title;
}

function classify(
  event: CalendarEventDTO,
  today: CivilDate,
  startDate: CivilDate,
  lastDate: CivilDate,
): Trip | null {
  const daysUntil = civilDaysBetween(today, startDate);
  const daysToLast = civilDaysBetween(today, lastDate);
  const lengthDays = civilDaysBetween(startDate, lastDate);
  if (daysUntil === null || daysToLast === null || lengthDays === null) {
    return null;
  }

  let phase: TripPhase;
  if (daysToLast < 0) phase = "past";
  else if (daysUntil > 0) phase = "upcoming";
  else if (daysUntil === 0) phase = "departs_today";
  else if (daysToLast === 0) phase = "returns_today";
  else phase = "under_way";

  return {
    id: event.providerEventId,
    name: tripName(event.title),
    location: event.location,
    startDate,
    lastDate,
    startMs: event.start.instantMs,
    endMs: event.end.instantMs,
    phase,
    daysUntil,
    lengthDays: lengthDays + 1,
    dayOfTrip: phase === "upcoming" || phase === "past" ? 0 : -daysUntil + 1,
  };
}

/** One calendar event read as a trip, or `null` if it cannot be read as one.
 *
 * `null` covers exactly one thing the reader should not be told about: an
 * **unusable zone**. `""` is a real value on the wire (`protocol.ts`) and
 * `Intl.DateTimeFormat` throws a `RangeError` on it — so the event is dropped
 * rather than resolved against a guessed zone, which would move the whole
 * trip by up to a day. `zoned-day.ts`'s own rule. */
export function tripFromEvent(event: CalendarEventDTO, nowMs: number): Trip | null {
  const today = deviceCivilToday(nowMs);
  const startDate = civilDateInZone(event.start.instantMs, event.start.timeZone);
  const endExclusive = civilDateInZone(event.end.instantMs, event.end.timeZone);
  if (today === null || startDate === null || endExclusive === null) {
    return null;
  }
  // The provider's end is EXCLUSIVE — local midnight after the last day — so
  // the last day is the day before it, ON THE CALENDAR.
  const lastDate = addCivilDays(endExclusive, -1);
  if (lastDate === null) {
    return null;
  }
  return classify(event, today, startDate, lastDate);
}

/** Every trip still ahead of (or under) today, soonest first.
 *
 * **Every non-cancelled event on the bound calendar is a trip** — all-day or
 * timed, no filter and no merging (#121 §4). A pane that decided some events
 * on the Trips calendar are not trips has started keeping a vacation record
 * of its own; the flight-plus-trip duplicate-row case is operator discipline
 * (one event per trip), not an invisible merge heuristic that hides a trip the
 * first time it guesses wrong. */
export function tripQueue(
  events: readonly CalendarEventDTO[],
  calendarId: string,
  nowMs: number,
): Trip[] {
  return events
    .filter((event) => event.calendarId === calendarId && event.status !== "cancelled")
    .map((event) => tripFromEvent(event, nowMs))
    .filter((trip): trip is Trip => trip !== null && trip.phase !== "past")
    .sort((left, right) => left.startDate.localeCompare(right.startDate) || left.id.localeCompare(right.id));
}

/** How soon the answer matters. `dormant` is reserved for "nothing to count
 * to" — never for "far away", which is what `distant` is for. */
export function vacationBand(next: Trip | null): Band {
  if (next === null) {
    return "dormant";
  }
  if (next.phase !== "upcoming") {
    return "live";
  }
  if (next.daysUntil <= IMMINENT_WITHIN_DAYS) {
    return "imminent";
  }
  if (next.daysUntil <= NEAR_WITHIN_DAYS) {
    return "near";
  }
  return "distant";
}

/** Epoch ms of this pane's next relevant moment — the next trip's start while
 * it is still ahead, the current trip's end once it is under way, and `null`
 * when nothing is booked (which sorts after every non-null). */
export function vacationWithinBand(next: Trip | null): number | null {
  if (next === null) {
    return null;
  }
  return next.phase === "upcoming" ? next.startMs : next.endMs;
}

/** The whole answer in one line. Place first — the question is about the
 * place, and leading with the number makes every trip read as a countdown to
 * an unnamed thing until the eye reaches the end of the line. */
export function vacationHeadline(next: Trip | null): string {
  if (next === null) {
    return `Nothing booked in the next ${HORIZON_LABEL}`;
  }
  switch (next.phase) {
    case "upcoming":
      return next.daysUntil === 1
        ? `${next.name} tomorrow`
        : `${next.name} in ${next.daysUntil} days`;
    case "departs_today":
      return `${next.name} today`;
    case "under_way":
      // Civil days like everything else, so it can never disagree with the
      // dates the expanded queue prints.
      return `In ${next.name} · day ${next.dayOfTrip} of ${next.lengthDays}`;
    case "returns_today":
      return `Home today from ${next.name}`;
    case "past":
      // Unreachable: `tripQueue` drops past trips. Answered here anyway
      // rather than left to fall through as `undefined`.
      return `${next.name} is over`;
  }
}

/** Whether the calendar read is old enough to say so. `"unknown"` is **never
 * fresh** — the prototype's `staleness(null, …) → { stale: false }` is the
 * bug the Rust carve-out exists to prevent. */
export function isStaleFreshness(freshness: FreshnessDTO): boolean {
  return freshness.kind === "unknown" || freshness.ageMs > STALE_AFTER_MS;
}

function vacationRead(inputs: QuestionInputs): CalendarReadDTO | undefined {
  return inputs.calendarReads[CALENDAR_REQUEST_KEY];
}

/** Why this pane has no answer, or that it has one. Resolved once, so the
 * words a gap renders and the decision to be a gap cannot disagree. */
export type VacationSetup =
  | { kind: "no-calendar" }
  | { kind: "unbound" }
  | { kind: "unread" }
  | { kind: "bound"; calendarId: string; read: Extract<CalendarReadDTO, { state: "read" }> };

/** **`calendarConnected` is checked first** (#122's rule, carried over and
 * extended): "no calendar at all" and "no Trips calendar designated" are two
 * different missing steps, and the earlier one wins. A connected device whose
 * read has not landed — never polled, offline, sitting on `needsReconnect` —
 * is neither: it is waiting. */
export function vacationSetup(inputs: QuestionInputs): VacationSetup {
  if (!inputs.calendarConnected) {
    return { kind: "no-calendar" };
  }
  const calendarId = tripsCalendarId(inputs.bindings);
  if (calendarId === null) {
    // An unread bindings table lands here too, and deliberately: this pane's
    // *other* unbound reason (no calendar connected) has already been ruled
    // out, so the device is connected and simply has no trips calendar to
    // read yet. `vacationSetupIsUnread` below is what tells the expanded
    // rendering to say "checking" rather than "designate one".
    return inputs.bindings === null ? { kind: "unread" } : { kind: "unbound" };
  }
  const read = vacationRead(inputs);
  if (read === undefined || read.state === "not_read") {
    return { kind: "unread" };
  }
  return { kind: "bound", calendarId, read };
}

/** The answered view an expanded pane draws — `null` for every gap state,
 * mirroring `waste.ts`'s `wasteView` and `weekend.ts`'s `weekendView`. */
export interface VacationView {
  /** The soonest unfinished trip — the one the headline is about. `null`
   * when nothing is booked inside the horizon, which is still an *answer*. */
  next: Trip | null;
  /** Every trip after `next`, in order, **never truncated**: a "+1 more"
   * would be the pane withholding something it already has in hand. */
  later: Trip[];
  freshness: FreshnessDTO;
  /** Stale states its age and still answers — the `ContextTile` posture. */
  stale: boolean;
}

export function vacationView(inputs: QuestionInputs): VacationView | null {
  const setup = vacationSetup(inputs);
  if (setup.kind !== "bound") {
    return null;
  }
  const trips = tripQueue(setup.read.events, setup.calendarId, inputs.nowMs);
  return {
    next: trips[0] ?? null,
    later: trips.slice(1),
    freshness: setup.read.freshness,
    stale: isStaleFreshness(setup.read.freshness),
  };
}

/** This question's answer for the shell.
 *
 * **No glyphs.** Glyphs exist for a pane answering about several distinct
 * subjects at once (the waste pane's bins); this question has one subject and
 * its answer is already a sentence. */
export function vacationAnswer(inputs: QuestionInputs): PaneAnswer {
  const setup = vacationSetup(inputs);
  if (setup.kind === "no-calendar" || setup.kind === "unbound") {
    return {
      answerState: "unbound",
      band: "dormant",
      withinBand: null,
      collapsedHeadline: "Not set up",
    };
  }
  if (setup.kind === "unread") {
    return {
      answerState: "bound-but-unacquired",
      band: "dormant",
      withinBand: null,
      collapsedHeadline: "Waiting for the first calendar sync",
    };
  }

  const view = vacationView(inputs);
  const next = view?.next ?? null;
  return {
    answerState: "answered",
    band: vacationBand(next),
    withinBand: vacationWithinBand(next),
    collapsedHeadline: vacationHeadline(next),
  };
}

/** The pane's own calendar-arm request (#267): the long horizon, exactly the
 * window `CalendarHorizon::Long` polls, so the read never asks for an
 * interval the mirror was never filled for. */
export function vacationCalendarInterval(nowMs: number): { startMs: number; endMs: number } {
  return {
    startMs: nowMs - HORIZON_BEFORE_DAYS * DAY_MS,
    endMs: nowMs + HORIZON_AHEAD_DAYS * DAY_MS,
  };
}

/** "14–28 Mar" · "28 Mar – 3 Apr", with a year on any date outside the
 * current one — trips run years out, and a bare "9 Sep" on a 2027 trip is
 * indistinguishable from one this September. Built from the trip's own civil
 * dates, never from its instants. */
export function tripDateRange(trip: Trip, nowMs: number): string {
  const start = civilParts(trip.startDate);
  const last = civilParts(trip.lastDate);
  const thisYear = deviceCivilToday(nowMs)?.slice(0, 4) ?? "";
  const year = last.year === thisYear ? "" : ` ${last.year}`;
  if (start.month === last.month && start.year === last.year) {
    return `${start.day}–${last.day} ${last.monthName}${year}`;
  }
  return `${start.day} ${start.monthName} – ${last.day} ${last.monthName}${year}`;
}

const MONTH_NAMES = [
  "Jan",
  "Feb",
  "Mar",
  "Apr",
  "May",
  "Jun",
  "Jul",
  "Aug",
  "Sep",
  "Oct",
  "Nov",
  "Dec",
];

function civilParts(date: CivilDate): {
  year: string;
  month: string;
  monthName: string;
  day: string;
} {
  const [year, month, day] = date.split("-");
  return {
    year,
    month,
    monthName: MONTH_NAMES[Number(month) - 1] ?? month,
    day: String(Number(day)),
  };
}

/** "Today" · "Tomorrow" · "Friday" · "14 Mar" · "9 Sep 2027" — the queue's
 * right-hand column. Same civil-date arithmetic as everything else here. */
export function tripDayLabel(trip: Trip, nowMs: number): string {
  const today = deviceCivilToday(nowMs);
  const days = today === null ? null : civilDaysBetween(today, trip.startDate);
  if (days === 0) return "Today";
  if (days === 1) return "Tomorrow";
  if (days !== null && days > 1 && days < 7) {
    // Resolved as a UTC instant purely to name a weekday for a civil date —
    // no zone question is being asked here, and no count depends on it.
    return new Date(`${trip.startDate}T00:00:00Z`).toLocaleDateString("en-US", {
      weekday: "long",
      timeZone: "UTC",
    });
  }
  const parts = civilParts(trip.startDate);
  const thisYear = today?.slice(0, 4) ?? "";
  return parts.year === thisYear
    ? `${parts.day} ${parts.monthName}`
    : `${parts.day} ${parts.monthName} ${parts.year}`;
}
