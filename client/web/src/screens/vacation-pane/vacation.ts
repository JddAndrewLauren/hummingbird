import type { CalendarEventDTO, CalendarReadDTO, FreshnessDTO } from "../../store/protocol";
import { tripsCalendarId } from "../../calendar/selection";
import type { Band, PaneAnswer, QuestionInputs } from "../questions/contract";
import {
  addCivilDays,
  civilDaysBetween,
  deviceCivilToday,
  isCivilDate,
  type CivilDate,
} from "../waste-pane/zoned-day";
import { resolveZoneFacts } from "../questions/zone-bridge";
import {
  tripQueueFromCore,
  vacationAnswerFromCore,
  vacationBandFromCore,
  vacationSetupFromCore,
  vacationZoneQueriesFromCore,
  vacationViewFromCore,
  type PaneInputsSource,
  type TripCore,
  type TripPhaseCore,
  type ZoneFacts,
} from "../../decisions/seam";

// **How long to the next vacation** (#121, ADR-0015), answered over #245's
// pane shell — and since #534, **the web's rendering half of it only**.
//
// Every rule this file used to hold is now
// `hummingbird_core::decisions::panes::vacation`: the civil-date trip
// classification (`trip_from_event`/`classify`), `trip_queue`,
// `vacation_band`/`vacation_within_band`, `vacation_setup` and the gap
// kinds — resolved through the zone bridge's `DEVICE_ZONE` sentinel (the
// reader's own zone, never a payload-carried one) rather than raw `Date`
// math.
//
// **`Trip.name` does not sink.** `vacation.rs`'s `Trip` carries no `name`
// field — nothing in the core's own decision reads it, only
// `vacationHeadline`/the expanded queue do, so it would be exactly the
// "re-crossing a DTO field no rule reads" violation `inputs.rs` forbids.
// This file recovers it locally, by matching a core `Trip`'s `id` back to
// the `CalendarEventDTO` it came from and running `tripName` on its title
// — the one join this module still does.
//
// `tripDateRange`/`tripDayLabel`/`vacationHeadline`/`MONTH_NAMES` are
// rendering and stay here unchanged.

export const SUBJECT_KEY = "next-trip";
export const CALENDAR_REQUEST_KEY = "vacation";

const DAY_MS = 24 * 60 * 60 * 1000;

export const HORIZON_BEFORE_DAYS = 7;
export const HORIZON_AHEAD_DAYS = 730;
export const HORIZON_LABEL = "2 years";
export const STALE_AFTER_MS = 24 * 60 * 60 * 1000;

export type TripPhase = TripPhaseCore;

export interface Trip {
  id: string;
  name: string;
  location: string | null;
  startDate: CivilDate;
  lastDate: CivilDate;
  startMs: number;
  endMs: number;
  phase: TripPhase;
  daysUntil: number;
  lengthDays: number;
  dayOfTrip: number;
}

/** "Trip: India" and "Holiday — India" both read as "India". */
export function tripName(title: string): string {
  return title.replace(/^\s*(trip|holiday)\s*[:—-]\s*/i, "").trim() || title;
}

/** Recovers `Trip.name` by matching a core trip's `id` back to the event it
 * came from — the one join this module still does; see the module
 * header. */
function withName(trip: TripCore, events: readonly CalendarEventDTO[]): Trip {
  const source = events.find((event) => event.providerEventId === trip.id);
  return { ...trip, name: source ? tripName(source.title) : trip.id };
}

/** Every `(zone, civil-date)` fact [`tripQueue`]/[`vacationView`] need,
 * given only the events and the clock — a standalone caller's own
 * synthetic `PaneInputsSource`, since `vacation_zone_queries` reads a
 * whole `PaneInputs` to find the bound calendar's events. */
function zoneFactsFor(events: readonly CalendarEventDTO[], calendarId: string, nowMs: number): ZoneFacts {
  const source: PaneInputsSource = {
    nowMs,
    bindings: [{ key: "trips-calendar", known: true, pending: false, value: { state: "text", text: calendarId } }],
    paneReads: {},
    calendarConnected: true,
    calendarReads: {
      [CALENDAR_REQUEST_KEY]: {
        state: "read",
        events: events as CalendarEventDTO[],
        freshness: { kind: "age", ageMs: 0, declaredCadenceMs: null },
      },
    },
  };
  return resolveZoneFacts(vacationZoneQueriesFromCore(source));
}

/** Every trip still ahead of (or under) today, soonest first —
 * `vacation.rs`'s `trip_queue`, resolving the bridge for exactly this
 * call (`waste.ts`'s own `resolve()` shape) since this is a standalone
 * convenience export, not fed through the shell's `QuestionInputs`. */
export function tripQueue(
  events: readonly CalendarEventDTO[],
  calendarId: string,
  nowMs: number,
): Trip[] {
  const today = deviceCivilToday(nowMs);
  if (today === null) {
    return [];
  }
  const facts = zoneFactsFor(events, calendarId, nowMs);
  const trips = tripQueueFromCore(events as CalendarEventDTO[], calendarId, today, facts);
  return trips.map((trip) => withName(trip, events));
}

/** `vacation.rs`'s `vacation_band`. */
export function vacationBand(next: Trip | null): Band {
  return vacationBandFromCore(next);
}

/** The whole answer in one line. */
export function vacationHeadline(next: Trip | null): string {
  if (next === null) {
    return `Nothing booked in the next ${HORIZON_LABEL}`;
  }
  switch (next.phase) {
    case "upcoming":
      return next.daysUntil === 1 ? `${next.name} tomorrow` : `${next.name} in ${next.daysUntil} days`;
    case "departs_today":
      return `${next.name} today`;
    case "under_way":
      return `In ${next.name} · day ${next.dayOfTrip} of ${next.lengthDays}`;
    case "returns_today":
      return `Home today from ${next.name}`;
    case "past":
      return `${next.name} is over`;
  }
}

export function isStaleFreshness(freshness: FreshnessDTO): boolean {
  return freshness.kind === "unknown" || freshness.ageMs > STALE_AFTER_MS;
}

function vacationRead(inputs: QuestionInputs): CalendarReadDTO | undefined {
  return inputs.calendarReads[CALENDAR_REQUEST_KEY];
}

function paneInputs(inputs: QuestionInputs): PaneInputsSource {
  return {
    nowMs: inputs.nowMs,
    bindings: inputs.bindings,
    paneReads: inputs.paneReads,
    calendarReads: inputs.calendarReads,
    calendarConnected: inputs.calendarConnected,
    items: inputs.items,
  };
}

/** Why this pane has no answer, or that it has one — `vacation.rs`'s
 * `vacation_setup`, via its kind-only projection `vacation_setup_kind`
 * (`VacationSetup<'a>` itself cannot cross the seam: its `Bound` arm
 * borrows the inputs' own event slice, so it has no `Serialize`). The
 * `Bound` arm's `read` is attached here from the same `calendarReads` the
 * core already read to decide `Bound` in the first place — not a second
 * guess about its state, just the one field the projection could not
 * carry across. */
export type VacationSetup =
  | { kind: "no-calendar" }
  | { kind: "unbound" }
  | { kind: "unread" }
  | { kind: "bound"; calendarId: string; read: Extract<CalendarReadDTO, { state: "read" }> };

export function vacationSetup(inputs: QuestionInputs): VacationSetup {
  const core = vacationSetupFromCore(paneInputs(inputs));
  switch (core.kind) {
    case "noCalendar":
      return { kind: "no-calendar" };
    case "unbound":
      return { kind: "unbound" };
    case "unread":
      return { kind: "unread" };
    case "bound": {
      const read = vacationRead(inputs);
      if (read === undefined || read.state !== "read") {
        // Unreachable: `vacation_setup_kind` only answers `bound` once
        // `calendarReads[CALENDAR_REQUEST_KEY]` is a landed `"read"` —
        // the exact precedence `vacation.rs`'s own `vacation_setup`
        // decides. A mismatch here would mean the two disagreed about
        // that precedence, which is a bug worth a loud failure rather
        // than a silently invented gap.
        throw new Error("vacation_setup_kind answered bound with no landed calendar read");
      }
      return { kind: "bound", calendarId: core.calendarId, read };
    }
  }
}

export interface VacationView {
  next: Trip | null;
  later: Trip[];
  freshness: FreshnessDTO;
  stale: boolean;
}

/** The answered view an expanded pane draws — `null` for every gap state,
 * mirroring `waste.ts`'s `wasteView`. */
export function vacationView(inputs: QuestionInputs): VacationView | null {
  const calendarId = tripsCalendarId(inputs.bindings);
  const read = vacationRead(inputs);
  if (!inputs.calendarConnected || calendarId === null || read === undefined || read.state !== "read") {
    return null;
  }
  const source = paneInputs(inputs);
  const facts = resolveZoneFacts(vacationZoneQueriesFromCore(source));
  const resolved = vacationViewFromCore(source, facts);
  if (resolved === null || resolved.kind !== "facts") {
    return null;
  }
  return {
    next: resolved.next === null ? null : withName(resolved.next, read.events),
    later: resolved.later.map((trip) => withName(trip, read.events)),
    freshness: resolved.freshness,
    stale: resolved.stale,
  };
}

/** This question's answer for the shell. No glyphs: one subject, and the
 * answer is already a sentence. */
export function vacationAnswer(inputs: QuestionInputs): PaneAnswer {
  const source = paneInputs(inputs);
  const facts = resolveZoneFacts(vacationZoneQueriesFromCore(source));
  const answer = vacationAnswerFromCore(source, facts);

  if (answer.answerState === "unbound") {
    return { ...answer, collapsedHeadline: "Not set up" };
  }
  if (answer.answerState === "bound-but-unacquired") {
    return { ...answer, collapsedHeadline: "Waiting for the first calendar sync" };
  }

  const view = vacationView(inputs);
  const next = view?.next ?? null;
  return { ...answer, collapsedHeadline: vacationHeadline(next) };
}

/** The pane's own calendar-arm request (#267): the long horizon. */
export function vacationCalendarInterval(nowMs: number): {
  startMs: number;
  endMs: number;
  startDate: string;
  endDate: string;
} {
  const startMs = nowMs - HORIZON_BEFORE_DAYS * DAY_MS;
  const endMs = nowMs + HORIZON_AHEAD_DAYS * DAY_MS;
  const endDay = deviceCivilToday(endMs) ?? new Date(endMs).toISOString().slice(0, 10);
  const endDate = addCivilDays(endDay, 1);
  if (endDate === null) {
    throw new Error(`calendar horizon produced an invalid civil end day: ${endDay}`);
  }
  return {
    startMs,
    endMs,
    startDate: deviceCivilToday(startMs) ?? new Date(startMs).toISOString().slice(0, 10),
    endDate,
  };
}

/** "14–28 Mar" · "28 Mar – 3 Apr", with a year on any date outside the
 * current one. */
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

function civilParts(date: CivilDate): { year: string; month: string; monthName: string; day: string } {
  const [year, month, day] = date.split("-");
  return { year, month, monthName: MONTH_NAMES[Number(month) - 1] ?? month, day: String(Number(day)) };
}

/** "Today" · "Tomorrow" · "Friday" · "14 Mar" · "9 Sep 2027". */
export function tripDayLabel(trip: Trip, nowMs: number): string {
  const today = deviceCivilToday(nowMs);
  const days = today === null ? null : civilDaysBetween(today, trip.startDate);
  if (days === 0) return "Today";
  if (days === 1) return "Tomorrow";
  if (days !== null && days > 1 && days < 7) {
    return new Date(`${trip.startDate}T00:00:00Z`).toLocaleDateString("en-US", {
      weekday: "long",
      timeZone: "UTC",
    });
  }
  const parts = civilParts(trip.startDate);
  const thisYear = today?.slice(0, 4) ?? "";
  return parts.year === thisYear ? `${parts.day} ${parts.monthName}` : `${parts.day} ${parts.monthName} ${parts.year}`;
}

// Re-exported for callers that only need civil-date shape checks —
// unchanged behaviour, `zoned-day.ts`'s own export.
export { isCivilDate };
