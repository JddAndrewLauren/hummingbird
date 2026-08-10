// PROTOTYPE — throwaway. Delete with the rest of this directory (#119).
//
// The read-time answer. ADR-0002 verbatim: a standing question is answered
// fresh at read time over the mirror and never stored — so nothing here
// writes anything back onto a snapshot, and every function takes `nowMs`.

import type {
  RaceAlertRow,
  RaceScenario,
  RaceSession,
  RaceSnapshotRow,
} from "./fixture";

export interface SeriesAnswer {
  series: string;
  seriesLabel: string;
  feed: string;
  /** The row this was computed from, or null when the binding names a series
   * that has never polled — an honest gap, not an absent pane. */
  row: RaceSnapshotRow | null;
  /** The next thing to happen, across all sessions of all events. */
  next: {
    eventName: string;
    circuit: string;
    locality: string;
    session: RaceSession;
    /** ms until it starts; negative while it is under way. */
    deltaMs: number;
    underWay: boolean;
  } | null;
  /** The race itself, of the event `next` belongs to — what the headline
   * counts to. Separate from `next`, which is whatever happens soonest
   * (usually Friday practice, two days earlier). */
  race: { session: RaceSession; deltaMs: number; underWay: boolean } | null;
  staleness: Staleness;
  /** Alerts joined by `source` + key. The join the plan warns can break. */
  alerts: RaceAlertRow[];
}

export interface Staleness {
  /** "12m ago", "9h ago", null when there is no snapshot at all. */
  label: string | null;
  stale: boolean;
}

const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

/** Older than two poll intervals reads as stale — the same shape as the
 * calendar tile's "cadence + slack", scaled to a much slower cron. */
export function staleness(row: RaceSnapshotRow | null, nowMs: number): Staleness {
  if (!row) {
    return { label: null, stale: false };
  }
  const ageMs = Math.max(0, nowMs - row.fetchedAtMs);
  return { label: relative(ageMs), stale: ageMs > 2 * row.pollIntervalMs };
}

/** "just now" · "12m ago" · "9h ago" · "3d ago". */
export function relative(ageMs: number): string {
  if (ageMs < 90_000) return "just now";
  if (ageMs < HOUR) return `${Math.round(ageMs / MIN)}m ago`;
  if (ageMs < DAY) return `${Math.round(ageMs / HOUR)}h ago`;
  return `${Math.round(ageMs / DAY)}d ago`;
}

/** The headline number, split so a variant can set the value and its unit in
 * different type. "12 days", "4 hr", "90 min", "under way". `hr` and `min`
 * do not inflect — an abbreviation is already a machine value, and "1 mins"
 * is the only way to get this wrong. */
export function countdown(deltaMs: number): { value: string; unit: string } {
  if (deltaMs <= 0) return { value: "under", unit: "way" };
  const minutes = Math.round(deltaMs / MIN);
  // The boundary sits above the 90-minute alert lead deliberately: rounding
  // 90 minutes up to "2 hr" would have the pane and the alert it shares a
  // source with disagreeing about the same instant.
  if (minutes < 120) return { value: String(minutes), unit: "min" };
  const hours = Math.round(deltaMs / HOUR);
  if (hours < 36) return { value: String(hours), unit: "hr" };
  const days = Math.round(deltaMs / DAY);
  return { value: String(days), unit: days === 1 ? "day" : "days" };
}

/** The issue's own phrasing, and the decided one: the headline counts to
 * **race day** and names the **race**, never the practice session that
 * happens to come first. Friday practice is still shown — on the line under
 * the headline (`VariantA`), where it cannot be mistaken for the answer.
 *
 * A session already running is the one exception: while the cars are on
 * track, that is the truer answer than a countdown to Sunday. */
export function sentence(answer: SeriesAnswer): string {
  if (!answer.next) return "No scheduled sessions.";
  if (answer.next.underWay) {
    // The event is named even here: "Practice 1 under way" answers which
    // session but not which race, and the pane's whole job is the second
    // one — "Monaco Practice 1 under way".
    return `${shortName(answer.next.eventName)} ${answer.next.session.label} under way`;
  }
  const race = answer.race;
  if (!race) {
    // A schedule with sessions but no race (a feed that only published
    // practice, say) — say what there is rather than inventing a race day.
    const { value, unit } = countdown(answer.next.deltaMs);
    return `${answer.next.session.label} in ${value} ${unit}`;
  }
  const { value, unit } = countdown(race.deltaMs);
  return `${value} ${unit} before ${abbreviate(answer.next.eventName)}`;
}

/** The event name as rendered: "Monaco Grand Prix" reads as "Monaco GP".
 * Names with no Grand Prix in them ("Iowa 275") pass through untouched. */
export function abbreviate(eventName: string): string {
  return eventName.replace(/\s+Grand Prix$/i, " GP");
}

/** The locality alone — "Monaco" — for the one line that already carries the
 * session name next to it, where "Monaco GP Practice 3 under way" would put
 * two labels in front of the verb. */
export function shortName(eventName: string): string {
  return eventName.replace(/\s+(Grand Prix|GP)$/i, "");
}

// Every time on this pane is Pacific, stated as Pacific — never the device's
// own zone. A feed publishes UTC instants and the reader is on Pacific time;
// resolving to `undefined` (the browser default) would render the same race
// at a different hour on a travelling laptop, and silently, since nothing on
// screen would say which zone it had picked. The "PT" suffix is the same
// honesty habit as "as of 12m ago": say what the number is, do not make the
// reader assume. Hardcoded deliberately — see NOTES.md.
const ZONE = "America/Los_Angeles";

/** Pacific wall-clock label — "6:00 AM PT". A UTC start is not an answer to
 * "when". */
export function clock(atMs: number): string {
  const label = new Intl.DateTimeFormat("en-US", {
    timeZone: ZONE,
    hour: "numeric",
    minute: "2-digit",
  }).format(new Date(atMs));
  return `${label} PT`;
}

export function dayLabel(atMs: number, nowMs: number): string {
  // Compared as Pacific calendar days, not as elapsed milliseconds: a race at
  // 11pm Pacific and a "now" of 1am the next day are 2 hours apart and two
  // different days, and only the day comparison gets "Tomorrow" right.
  const days = pacificDayIndex(atMs) - pacificDayIndex(nowMs);
  if (days === 0) return "Today";
  if (days === 1) return "Tomorrow";
  if (days < 7) {
    return new Intl.DateTimeFormat("en-US", { timeZone: ZONE, weekday: "long" }).format(
      new Date(atMs),
    );
  }
  return new Intl.DateTimeFormat("en-US", {
    timeZone: ZONE,
    month: "short",
    day: "numeric",
  }).format(new Date(atMs));
}

/** Which Pacific calendar day an instant falls on, as a comparable integer. */
function pacificDayIndex(atMs: number): number {
  const parts = new Intl.DateTimeFormat("en-US", {
    timeZone: ZONE,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).formatToParts(new Date(atMs));
  const part = (type: string) => Number(parts.find((entry) => entry.type === type)?.value ?? 0);
  return Math.floor(Date.UTC(part("year"), part("month") - 1, part("day")) / DAY);
}

/** One answer per series named by the binding — in binding order, including
 * the series that have no row. */
export function answerAll(scenario: RaceScenario): SeriesAnswer[] {
  return scenario.seriesBinding.map((series) => {
    const row = scenario.snapshots.find((snapshot) => snapshot.key === series) ?? null;
    const alerts = scenario.alerts.filter(
      (alert) => alert.source === "race-schedule" && alert.sourceKey === series,
    );
    if (!row) {
      return {
        series,
        seriesLabel: series.toUpperCase(),
        feed: "",
        row: null,
        next: null,
        race: null,
        staleness: { label: null, stale: false },
        alerts,
      };
    }

    let next: SeriesAnswer["next"] = null;
    let race: SeriesAnswer["race"] = null;
    for (const event of row.payload.events) {
      for (const session of event.sessions) {
        // "Next" includes what is under way right now — a session in progress
        // is the truest answer to "when is the next race", and dropping it
        // would jump the pane to tomorrow while the cars are on track.
        if (session.endsAtMs <= scenario.nowMs) continue;
        const deltaMs = session.startsAtMs - scenario.nowMs;
        if (!next || deltaMs < next.deltaMs) {
          next = {
            eventName: event.name,
            circuit: event.circuit,
            locality: event.locality,
            session,
            deltaMs,
            underWay: session.startsAtMs <= scenario.nowMs,
          };
          // The headline's own target: this event's race, not the soonest
          // session. Falls back to null rather than to the last session —
          // guessing which practice "counts as" the race would be a lie the
          // countdown then repeats every render.
          const raceSession =
            event.sessions.find(
              (entry) => entry.kind === "race" && entry.endsAtMs > scenario.nowMs,
            ) ?? null;
          race = raceSession
            ? {
                session: raceSession,
                deltaMs: raceSession.startsAtMs - scenario.nowMs,
                underWay: raceSession.startsAtMs <= scenario.nowMs,
              }
            : null;
        }
      }
    }

    return {
      series,
      seriesLabel: row.payload.seriesLabel,
      feed: row.payload.feed,
      row,
      next,
      race,
      staleness: staleness(row, scenario.nowMs),
      alerts,
    };
  });
}
