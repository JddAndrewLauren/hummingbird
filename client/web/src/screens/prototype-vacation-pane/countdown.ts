// PROTOTYPE — throwaway. Delete with the rest of this directory (#121).
//
// The read-time answer. ADR-0002 verbatim: a standing question is answered
// fresh at read time over the mirror and never stored — so nothing here
// writes back onto a snapshot, every function takes `nowMs`, and the whole
// module would still be correct if the mirror were deleted and re-pulled.
//
// Two things it refuses to do, both deliberate:
//
//   * Count in milliseconds. A trip is a RANGE OF LOCAL DAYS, not an
//     instant. `Math.round(deltaMs / 86_400_000)` gives the wrong day count
//     twice a year at a DST boundary and on any trip whose start is not
//     local midnight in the reader's own zone. Every count here walks local
//     midnights.
//   * Say "nothing booked" when what it means is "nothing I can see".
//     The mirror keeps a WINDOW (#46: −7d…+90d). Silence inside a window is
//     not silence outside it, and conflating them is the one failure mode
//     that would make this pane actively lie.

import type { TripEvent, TripSnapshot, VacationScenario } from "./fixture";

const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

/** Where a trip sits relative to today.
 *
 * `returns_today` exists because the issue's wording and the calendar
 * disagree: "the day you land home it is already counting to the next one"
 * says the trip is over, while the event is still live until tomorrow's
 * midnight. Rather than pick silently, the phase is named and every variant
 * gets to render it — see NOTES.md. */
export type TripPhase =
  | "upcoming"
  | "departs_today"
  | "under_way"
  | "returns_today"
  | "past";

export interface Trip {
  event: TripEvent;
  /** Trip title with any "Trip:" / "Holiday:" prefix stripped — the name the
   * countdown sentence says ("India"). */
  name: string;
  phase: TripPhase;
  /** Whole local days from today's midnight to the first day. 0 = today,
   * negative once it has started. */
  daysUntil: number;
  /** Total local days the trip covers, inclusive of both ends. */
  lengthDays: number;
  /** Days left including today, once it is under way; 0 otherwise. */
  daysLeft: number;
}

/** Why the pane has no countdown to give, or that it has one. Each of these
 * reads differently on screen and collapsing any two of them would hide a
 * real difference from the reader. */
export type AnswerState =
  | "unbound" // no Trips calendar designated (the slice's human step)
  | "not_polling" // bound, but this device never opted in (#46, per-device)
  | "no_snapshot" // opted in, never completed a poll
  | "none_in_horizon" // polled, empty — but only out to the window's end
  | "answered";

export interface Staleness {
  label: string | null;
  stale: boolean;
}

export interface VacationAnswer {
  state: AnswerState;
  /** The trip happening right now, if any. */
  current: Trip | null;
  /** The soonest trip that has not finished — equals `current` while one is
   * under way, which is what makes "auto-advance" fall out for free rather
   * than needing a rule. */
  next: Trip | null;
  /** Everything after `next`, in order, inside the window. */
  later: Trip[];
  /** How far ahead the mirror can actually see, as a whole-day count. */
  horizonDays: number;
  /** Local-midnight instant the window ends. */
  horizonEndMs: number;
  staleness: Staleness;
  calendarId: string | null;
}

/** Local midnight of the day `atMs` falls in. */
function startOfDay(atMs: number): number {
  const date = new Date(atMs);
  date.setHours(0, 0, 0, 0);
  return date.getTime();
}

/** Whole local days from `fromMs`'s day to `toMs`'s day. Walks the calendar
 * rather than dividing, so a DST transition inside the span does not shave or
 * add a day. */
export function daysBetween(fromMs: number, toMs: number): number {
  const from = startOfDay(fromMs);
  const to = startOfDay(toMs);
  // A day is 23, 24 or 25 hours; rounding the quotient is exact for spans of
  // any length because at most one hour of drift accumulates per transition
  // and two transitions never fall inside the same day.
  return Math.round((to - from) / DAY);
}

/** "just now" · "12m ago" · "3h ago" · "3d ago" — the design system's
 * relative-time register, unchanged from the calendar tile's. */
export function relative(ageMs: number): string {
  if (ageMs < 90_000) return "just now";
  if (ageMs < HOUR) return `${Math.round(ageMs / MIN)}m ago`;
  if (ageMs < DAY) return `${Math.round(ageMs / HOUR)}h ago`;
  return `${Math.round(ageMs / DAY)}d ago`;
}

/** Older than two poll intervals reads as stale — same shape as the calendar
 * tile's own "cadence + slack". */
export function staleness(
  snapshot: TripSnapshot | null,
  pollIntervalMs: number,
  nowMs: number,
): Staleness {
  if (!snapshot) return { label: null, stale: false };
  const ageMs = Math.max(0, nowMs - snapshot.fetchedAtMs);
  return { label: relative(ageMs), stale: ageMs > 2 * pollIntervalMs };
}

/** "Trip: India" and "Holiday — India" both read as "India" in a countdown
 * sentence. Anything else is left exactly as the human typed it: the calendar
 * is the authority, and rewriting its titles would be the pane inventing a
 * vacation record of its own. */
export function tripName(title: string): string {
  return title.replace(/^\s*(trip|holiday|vacation|hols)\s*[:—-]\s*/i, "").trim() || title;
}

function classify(event: TripEvent, nowMs: number): Trip {
  const name = tripName(event.title);
  const daysUntil = daysBetween(nowMs, event.startMs);
  // The provider's end is EXCLUSIVE — local midnight on the day after the
  // last day — so the last day of the trip is `endMs` minus one day.
  const lastDayMs = event.endMs - DAY;
  const lengthDays = daysBetween(event.startMs, lastDayMs) + 1;
  const daysToLast = daysBetween(nowMs, lastDayMs);

  let phase: TripPhase;
  if (daysToLast < 0) phase = "past";
  else if (daysUntil > 0) phase = "upcoming";
  else if (daysUntil === 0) phase = "departs_today";
  else if (daysToLast === 0) phase = "returns_today";
  else phase = "under_way";

  return {
    event,
    name,
    phase,
    daysUntil,
    lengthDays,
    daysLeft: phase === "past" || phase === "upcoming" ? 0 : daysToLast + 1,
  };
}

/** The whole answer, from the three inputs and nothing else. */
export function answer(scenario: VacationScenario): VacationAnswer {
  const { nowMs, snapshot } = scenario;
  const stale = staleness(snapshot, scenario.pollIntervalMs, nowMs);
  const horizonEndMs = snapshot?.windowEndMs ?? nowMs;
  const base: Omit<VacationAnswer, "state"> = {
    current: null,
    next: null,
    later: [],
    horizonDays: snapshot ? daysBetween(nowMs, horizonEndMs) : 0,
    horizonEndMs,
    staleness: stale,
    calendarId: scenario.binding,
  };

  if (!scenario.binding) return { ...base, state: "unbound" };
  if (!scenario.polling) return { ...base, state: "not_polling" };
  if (!snapshot) return { ...base, state: "no_snapshot" };

  const trips = snapshot.events
    .map((event) => classify(event, nowMs))
    // "Auto-advances past finished events" is this filter and nothing else.
    // A trip drops out the moment its last day is behind us, so the pane
    // re-answers itself with no rule, no write and no stored cursor — which
    // is the whole reason the acceptance criterion is satisfiable at all.
    .filter((trip) => trip.phase !== "past")
    .sort((left, right) => left.event.startMs - right.event.startMs);

  if (trips.length === 0) return { ...base, state: "none_in_horizon" };

  const next = trips[0];
  return {
    ...base,
    state: "answered",
    current: next.phase === "upcoming" ? null : next,
    next,
    later: trips.slice(1),
  };
}

/** The headline, split so a variant can set value and unit in different type.
 * Only ever called for an `upcoming` trip; the other phases do not have a
 * countdown to render and say something else entirely. */
export function countdown(daysUntil: number): { value: string; unit: string } {
  return { value: String(daysUntil), unit: daysUntil === 1 ? "day" : "days" };
}

/** The decided phrasing — **place first**: "Lisbon in 16 days", not the
 * issue's "16 days before Lisbon". The question is about the place; the
 * number is only how far away it is, and leading with it makes every trip
 * read as a countdown to an unnamed thing until the eye reaches the end of
 * the line. The four non-`upcoming` phases each get their own sentence
 * because a countdown is meaningless in all of them. */
export function sentence(trip: Trip): string {
  switch (trip.phase) {
    case "upcoming": {
      const { value, unit } = countdown(trip.daysUntil);
      return `${trip.name} in ${value} ${unit}`;
    }
    case "departs_today":
      return `${trip.name} starts today`;
    case "under_way":
      return `In ${trip.name} — ${trip.daysLeft} days left`;
    case "returns_today":
      return `Home today from ${trip.name}`;
    case "past":
      return `${trip.name} is over`;
  }
}

/** What the pane says when it has no trip: never "nothing booked" unless the
 * window is genuinely wide enough for that to be a fact. */
export function emptySentence(answer: VacationAnswer): string {
  switch (answer.state) {
    case "unbound":
      return "No Trips calendar designated.";
    case "not_polling":
      return "The Trips calendar is not polled on this device.";
    case "no_snapshot":
      return "Never polled. Nothing to answer with yet.";
    case "none_in_horizon":
      return `Nothing booked in the next ${answer.horizonDays} days.`;
    case "answered":
      return "";
  }
}

/** Date range in the reader's own wall clock. "14–28 Mar" · "28 Mar – 3 Apr". */
export function dateRange(trip: Trip): string {
  const start = new Date(trip.event.startMs);
  const last = new Date(trip.event.endMs - DAY);
  const day = (date: Date) => date.getDate();
  const month = (date: Date) => date.toLocaleDateString(undefined, { month: "short" });
  const year = (date: Date) =>
    date.getFullYear() === new Date().getFullYear() ? "" : ` ${date.getFullYear()}`;
  if (start.getMonth() === last.getMonth() && start.getFullYear() === last.getFullYear()) {
    return `${day(start)}–${day(last)} ${month(last)}${year(last)}`;
  }
  return `${day(start)} ${month(start)} – ${day(last)} ${month(last)}${year(last)}`;
}

/** "Today" · "Tomorrow" · "Friday" · "14 Mar" · "9 Sep 2027" — the same
 * ladder the design system's content rules use for relative dates, with one
 * addition the rest of the app does not need: trips run years out, and a
 * bare "9 Sep" on a 2027 trip is indistinguishable from one this September.
 * The year appears only when it is not the current one. */
export function dayLabel(atMs: number, nowMs: number): string {
  const days = daysBetween(nowMs, atMs);
  if (days === 0) return "Today";
  if (days === 1) return "Tomorrow";
  if (days > 1 && days < 7) {
    return new Date(atMs).toLocaleDateString(undefined, { weekday: "long" });
  }
  const date = new Date(atMs);
  // The year is appended rather than asked of the formatter: with `year` in
  // the options most locales punctuate it ("Sep 9, 2027"), and a comma in an
  // 11px uppercase mono line is noise the rest of these labels do not carry.
  const label = date.toLocaleDateString(undefined, { day: "numeric", month: "short" });
  const sameYear = date.getFullYear() === new Date(nowMs).getFullYear();
  return sameYear ? label : `${label} ${date.getFullYear()}`;
}
