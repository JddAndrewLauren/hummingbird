// PROTOTYPE — throwaway. Delete with the rest of this directory (#121).
//
// Fixture data shaped like what #121 will really have. Three inputs, no more:
//
//   1. the `trips.calendar_id` binding (#118's `settings` KV row),
//   2. whether THIS device opted that calendar into polling (#46: context is
//      per-device opt-in; a device with no consent has no calendar at all),
//   3. the calendar mirror's snapshot — its `fetched_at`, its window, and the
//      events inside it.
//
// There is deliberately no fourth input. #121's whole point is that the
// calendar stays the authority and no vacation record exists anywhere in the
// task domain, so nothing here has an id, a stage, or a version.
//
// `TripEvent` is a narrowing of the core's real `EventRecord`
// (`client/core/src/calendar/event.rs`) down to the fields a countdown could
// want. The all-day boundary convention is the real one and it matters here:
// `startMs` is local midnight on the first day, `endMs` is local midnight on
// the day AFTER the last day (the provider's exclusive end).

export interface TripEvent {
  providerEventId: string;
  title: string;
  /** Local-midnight instant of the first day of the trip. */
  startMs: number;
  /** EXCLUSIVE: local-midnight instant of the day after the last day. */
  endMs: number;
  allDay: boolean;
  location: string | null;
  htmlLink: string | null;
}

/** The calendar mirror's rolling snapshot, narrowed. `windowEndMs` is the
 * load-bearing field — #46 persists seven days back through **ninety days
 * ahead**, and a pane that ignores that window will report "nothing booked"
 * for a trip it simply cannot see. */
export interface TripSnapshot {
  fetchedAtMs: number;
  windowStartMs: number;
  windowEndMs: number;
  events: TripEvent[];
}

export interface VacationScenario {
  key: string;
  label: string;
  /** What this scenario is here to expose. */
  note: string;
  nowMs: number;
  /** The `trips.calendar_id` binding; null when nothing is designated. */
  binding: string | null;
  /** Whether this device opted the bound calendar into polling. */
  polling: boolean;
  /** null when the device has never completed a poll. */
  snapshot: TripSnapshot | null;
  /** The calendar poll cadence; staleness is judged against it (#46: 15m). */
  pollIntervalMs: number;
}

const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;
const NOW = Date.now();
const POLL = 15 * MIN;
const CALENDAR = "trips@group.calendar.google.com";

/** Local midnight `days` from today — not `NOW + days * DAY`. An all-day
 * boundary names a local day, and DST would slide a ms-arithmetic countdown
 * by an hour twice a year, which is exactly enough to flip a day count. */
function midnight(days: number): number {
  const date = new Date(NOW);
  date.setHours(0, 0, 0, 0);
  date.setDate(date.getDate() + days);
  return date.getTime();
}

function trip(
  id: string,
  title: string,
  location: string,
  startDay: number,
  nights: number,
): TripEvent {
  return {
    providerEventId: id,
    title,
    startMs: midnight(startDay),
    endMs: midnight(startDay + nights),
    allDay: true,
    location,
    htmlLink: `https://calendar.google.com/event?eid=${id}`,
  };
}

/** The default window the mirror actually keeps: −7d through +90d (#46). */
function narrowWindow(fetchedAtMs: number, events: TripEvent[]): TripSnapshot {
  return {
    fetchedAtMs,
    windowStartMs: midnight(-7),
    windowEndMs: midnight(90),
    events,
  };
}

/** The window #121 would need instead — wide enough to see a trip a year out.
 * Not a thing that exists today; it is the scenario pair below that makes the
 * case for it. */
function wideWindow(fetchedAtMs: number, events: TripEvent[]): TripSnapshot {
  return {
    fetchedAtMs,
    windowStartMs: midnight(-7),
    windowEndMs: midnight(3 * 365),
    events,
  };
}

const INDIA = trip("evt-india", "India", "Kerala, India", 395, 18);
const LISBON = trip("evt-lisbon", "Lisbon", "Lisbon, Portugal", 16, 6);
const SNOWDONIA = trip("evt-snow", "Snowdonia", "Gwynedd, Wales", 61, 4);

export const SCENARIOS: VacationScenario[] = [
  {
    key: "far-narrow",
    label: "395 days out (real horizon)",
    note:
      "The issue's own example — India, 395 days out — against the mirror #46 actually keeps (+90d). The trip is REAL and the pane cannot see it. What it must not say is 'nothing booked'.",
    nowMs: NOW,
    binding: CALENDAR,
    polling: true,
    snapshot: narrowWindow(NOW - 6 * MIN, []),
    pollIntervalMs: POLL,
  },
  {
    key: "far-wide",
    label: "395 days out (wide horizon)",
    note:
      "The same trip with a window wide enough to hold it. This is the scenario the issue assumes exists; the pair above/below is the whole argument for widening the window.",
    nowMs: NOW,
    binding: CALENDAR,
    polling: true,
    snapshot: wideWindow(NOW - 6 * MIN, [INDIA]),
    pollIntervalMs: POLL,
  },
  {
    key: "queue",
    label: "Three booked",
    note:
      "Lisbon in 16 days, Snowdonia in 61, India in 395. Does the pane answer with one number, or with the shape of the year?",
    nowMs: NOW,
    binding: CALENDAR,
    polling: true,
    snapshot: wideWindow(NOW - 3 * MIN, [LISBON, SNOWDONIA, INDIA]),
    pollIntervalMs: POLL,
  },
  {
    key: "tomorrow",
    label: "Departing tomorrow",
    note: "One day out. '1 day before Lisbon' or 'Tomorrow'? The number stops being the point.",
    nowMs: NOW,
    binding: CALENDAR,
    polling: true,
    snapshot: wideWindow(NOW - 2 * MIN, [
      trip("evt-lisbon", "Lisbon", "Lisbon, Portugal", 1, 6),
      SNOWDONIA,
      INDIA,
    ]),
    pollIntervalMs: POLL,
  },
  {
    key: "underway",
    label: "Under way",
    note:
      "Day 3 of 6 in Lisbon. The issue never says what the pane reads DURING a trip — a countdown to the next one is absurd here.",
    nowMs: NOW,
    binding: CALENDAR,
    polling: true,
    snapshot: wideWindow(NOW - 4 * MIN, [
      trip("evt-lisbon", "Lisbon", "Lisbon, Portugal", -2, 6),
      SNOWDONIA,
      INDIA,
    ]),
    pollIntervalMs: POLL,
  },
  {
    key: "landing",
    label: "Landing today",
    note:
      "The last day of the trip. The issue says 'the day you land home it is already counting to the next one' — but the calendar still has the event live today. Which one wins?",
    nowMs: NOW,
    binding: CALENDAR,
    polling: true,
    snapshot: wideWindow(NOW - 4 * MIN, [
      trip("evt-lisbon", "Lisbon", "Lisbon, Portugal", -5, 6),
      SNOWDONIA,
      INDIA,
    ]),
    pollIntervalMs: POLL,
  },
  {
    key: "just-back",
    label: "Just back",
    note:
      "Lisbon ended yesterday and is still inside the mirror's −7d tail. The pane must auto-advance past it without a human touching anything.",
    nowMs: NOW,
    binding: CALENDAR,
    polling: true,
    snapshot: wideWindow(NOW - 4 * MIN, [
      trip("evt-lisbon", "Lisbon", "Lisbon, Portugal", -7, 6),
      SNOWDONIA,
      INDIA,
    ]),
    pollIntervalMs: POLL,
  },
  {
    key: "stale",
    label: "Stale mirror",
    note: "Three days since the last successful poll against a 15-minute cadence. Keep showing it, say its age.",
    nowMs: NOW,
    binding: CALENDAR,
    polling: true,
    snapshot: wideWindow(NOW - 3 * DAY, [SNOWDONIA, INDIA]),
    pollIntervalMs: POLL,
  },
  {
    key: "nothing",
    label: "Nothing booked",
    note: "A wide window, polled just now, genuinely empty. The one case where 'nothing booked' is honest.",
    nowMs: NOW,
    binding: CALENDAR,
    polling: true,
    snapshot: wideWindow(NOW - 90_000, []),
    pollIntervalMs: POLL,
  },
  {
    key: "unpolled",
    label: "Not polling here",
    note:
      "The Trips calendar is bound (it synced from another device) but this device never opted it into polling. Not the same as empty.",
    nowMs: NOW,
    binding: CALENDAR,
    polling: false,
    snapshot: null,
    pollIntervalMs: POLL,
  },
  {
    key: "unbound",
    label: "No Trips calendar",
    note: "The binding is empty — the human step in the slice was never done. Should the pane exist at all?",
    nowMs: NOW,
    binding: null,
    polling: false,
    snapshot: null,
    pollIntervalMs: POLL,
  },
];

export function scenarioByKey(key: string | null): VacationScenario {
  return SCENARIOS.find((scenario) => scenario.key === key) ?? SCENARIOS[0];
}
