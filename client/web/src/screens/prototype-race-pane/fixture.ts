// PROTOTYPE — throwaway. Delete with the rest of this directory (#119).
//
// Fixture data shaped like what #119 will really have: one
// `context_snapshots` row per followed series (`source`, `key`, `payload`,
// `fetched_at`), the series list read from the settings binding rather than
// code, and the race-start alert the same cron raises.
//
// The payload shape here is a GUESS, deliberately. #119's own acceptance
// criterion says the schedule source must be verified at wiring time before
// a payload shape is committed to; nothing in this directory decides that.
// What it is chosen to be is the *union of what a pane could plausibly want*
// — event name, circuit, locality, and a session ladder with UTC starts —
// so the variants can be judged on what they'd need rather than on what one
// arbitrary feed happens to give.

export type SessionKind = "practice" | "qualifying" | "sprint" | "race";

export interface RaceSession {
  kind: SessionKind;
  /** Feed's own label, e.g. "Practice 1". */
  label: string;
  startsAtMs: number;
  endsAtMs: number;
}

export interface RaceEvent {
  /** "Monaco Grand Prix" — the name the countdown says. */
  name: string;
  /** "Circuit de Monaco". */
  circuit: string;
  /** "Monte Carlo, Monaco". */
  locality: string;
  sessions: RaceSession[];
}

/** The `payload` column, parsed. Replaced wholesale each poll. */
export interface RaceSnapshotPayload {
  series: string;
  seriesLabel: string;
  /** Which feed this came from — documented per the issue's last criterion. */
  feed: string;
  events: RaceEvent[];
}

/** One `context_snapshots` row. */
export interface RaceSnapshotRow {
  /** The join key: an alert from this source must carry this exact string. */
  source: string;
  key: string;
  payload: RaceSnapshotPayload;
  fetchedAtMs: number;
  /** How often the cron polls this source; staleness is judged against it. */
  pollIntervalMs: number;
}

/** An `alerts` row raised by the same cron, joined to a snapshot by `source`. */
export interface RaceAlertRow {
  source: string;
  sourceKey: string;
  title: string;
  detail: string;
  raisedAtMs: number;
  severity: "urgent" | "normal";
}

export interface RaceScenario {
  key: string;
  label: string;
  /** What this scenario is here to expose. */
  note: string;
  nowMs: number;
  /** The `race.series` binding (#118) — the pane iterates THIS, not a literal. */
  seriesBinding: string[];
  snapshots: RaceSnapshotRow[];
  alerts: RaceAlertRow[];
}

const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

// A fixed instant so every scenario is reproducible and the countdown text is
// stable across reloads: 2026-08-10T09:00:00Z.
const NOW = Date.UTC(2026, 7, 10, 9, 0, 0);

const SOURCE = "race-schedule";

function session(
  kind: SessionKind,
  label: string,
  startsAtMs: number,
  durationMs: number,
): RaceSession {
  return { kind, label, startsAtMs, endsAtMs: startsAtMs + durationMs };
}

/** A race start `days` out, at `utcHour` — real feeds publish UTC instants,
 * and every session below hangs off this one. */
function raceAt(days: number, utcHour: number): number {
  const date = new Date(NOW + days * DAY);
  return Date.UTC(
    date.getUTCFullYear(),
    date.getUTCMonth(),
    date.getUTCDate(),
    utcHour,
    0,
    0,
  );
}

/** A full race weekend hung off its race start. */
function weekend(name: string, circuit: string, locality: string, raceAtMs: number): RaceEvent {
  return {
    name,
    circuit,
    locality,
    sessions: [
      session("practice", "Practice 1", raceAtMs - 2 * DAY - 2 * HOUR, 60 * MIN),
      session("practice", "Practice 2", raceAtMs - 2 * DAY + 2 * HOUR, 60 * MIN),
      session("practice", "Practice 3", raceAtMs - DAY - 2 * HOUR, 60 * MIN),
      session("qualifying", "Qualifying", raceAtMs - DAY + HOUR, 60 * MIN),
      session("race", "Race", raceAtMs, 2 * HOUR),
    ],
  };
}

function f1(fetchedAtMs: number, ...events: RaceEvent[]): RaceSnapshotRow {
  return {
    source: SOURCE,
    key: "f1",
    fetchedAtMs,
    pollIntervalMs: 6 * HOUR,
    payload: {
      series: "f1",
      seriesLabel: "Formula 1",
      feed: "jolpi.ca/ergast (Ergast successor) — unverified, see NOTES.md",
      events,
    },
  };
}

function indycar(fetchedAtMs: number, ...events: RaceEvent[]): RaceSnapshotRow {
  return {
    source: SOURCE,
    key: "indycar",
    fetchedAtMs,
    pollIntervalMs: 6 * HOUR,
    payload: {
      series: "indycar",
      seriesLabel: "IndyCar",
      feed: "indycar.com ICS calendar — unverified, see NOTES.md",
      events,
    },
  };
}

const BINDING = ["f1", "indycar"];

export const SCENARIOS: RaceScenario[] = [
  {
    key: "far",
    label: "Quiet week",
    note: "Both series far out. The pane is answering, not interrupting.",
    nowMs: NOW,
    seriesBinding: BINDING,
    snapshots: [
      f1(
        NOW - 40 * MIN,
        weekend("Monaco Grand Prix", "Circuit de Monaco", "Monte Carlo, Monaco", raceAt(12, 13)),
        weekend("Spanish Grand Prix", "Circuit de Barcelona", "Barcelona, Spain", raceAt(26, 13)),
      ),
      indycar(
        NOW - 40 * MIN,
        weekend("Iowa 275", "Iowa Speedway", "Newton, Iowa", raceAt(4, 19)),
      ),
    ],
    alerts: [],
  },
  {
    key: "weekend",
    label: "Race weekend",
    note: "A session is under way now and the race is tomorrow — two live facts at once.",
    nowMs: NOW,
    seriesBinding: BINDING,
    snapshots: [
      f1(
        NOW - 12 * MIN,
        {
          name: "Monaco Grand Prix",
          circuit: "Circuit de Monaco",
          locality: "Monte Carlo, Monaco",
          sessions: [
            session("practice", "Practice 3", NOW - 25 * MIN, 60 * MIN),
            session("qualifying", "Qualifying", NOW + 4 * HOUR, 60 * MIN),
            session("race", "Race", NOW + DAY + 4 * HOUR, 2 * HOUR),
          ],
        },
        weekend("Spanish Grand Prix", "Circuit de Barcelona", "Barcelona, Spain", raceAt(15, 13)),
      ),
      indycar(
        NOW - 12 * MIN,
        weekend("Iowa 275", "Iowa Speedway", "Newton, Iowa", raceAt(4, 19)),
      ),
    ],
    alerts: [],
  },
  {
    key: "imminent",
    label: "Race in 90 minutes",
    note: "The threshold alert is live. Does the pane change, or only the alert lane?",
    nowMs: NOW,
    seriesBinding: BINDING,
    snapshots: [
      f1(NOW - 6 * MIN, {
        name: "Monaco Grand Prix",
        circuit: "Circuit de Monaco",
        locality: "Monte Carlo, Monaco",
        sessions: [
          session("qualifying", "Qualifying", NOW - DAY, 60 * MIN),
          session("race", "Race", NOW + 90 * MIN, 2 * HOUR),
        ],
      }),
      indycar(
        NOW - 6 * MIN,
        weekend("Iowa 275", "Iowa Speedway", "Newton, Iowa", raceAt(4, 19)),
      ),
    ],
    alerts: [
      {
        source: SOURCE,
        sourceKey: "f1",
        title: "Monaco GP starts in 90 min",
        detail: "Race · 11:30 AM PT · Circuit de Monaco",
        raisedAtMs: NOW - 30_000,
        severity: "urgent",
      },
    ],
  },
  {
    key: "stale",
    label: "Cron missed",
    note: "Snapshot is 9 hours old against a 6-hour poll. Keep showing it, say its age.",
    nowMs: NOW,
    seriesBinding: BINDING,
    snapshots: [
      f1(
        NOW - 9 * HOUR,
        weekend("Monaco Grand Prix", "Circuit de Monaco", "Monte Carlo, Monaco", raceAt(12, 13)),
      ),
      indycar(
        NOW - 9 * HOUR,
        weekend("Iowa 275", "Iowa Speedway", "Newton, Iowa", raceAt(4, 19)),
      ),
    ],
    alerts: [],
  },
  {
    key: "partial",
    label: "One series missing",
    note: "The binding names two series; only one has ever polled. The honest state.",
    nowMs: NOW,
    seriesBinding: BINDING,
    snapshots: [
      f1(
        NOW - 40 * MIN,
        weekend("Monaco Grand Prix", "Circuit de Monaco", "Monte Carlo, Monaco", raceAt(12, 13)),
      ),
    ],
    alerts: [],
  },
  {
    key: "empty",
    label: "Nothing polled yet",
    note: "Binding set, cron has never run. What does the pane say?",
    nowMs: NOW,
    seriesBinding: BINDING,
    snapshots: [],
    alerts: [],
  },
  {
    key: "unbound",
    label: "No series followed",
    note: "The binding is empty. The pane should not exist at all.",
    nowMs: NOW,
    seriesBinding: [],
    snapshots: [],
    alerts: [],
  },
];

export function scenarioByKey(key: string | null): RaceScenario {
  return SCENARIOS.find((scenario) => scenario.key === key) ?? SCENARIOS[0];
}
