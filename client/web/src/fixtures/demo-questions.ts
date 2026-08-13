import type { QuestionInputs } from "../screens/questions/contract";
import { SOURCE as GITHUB_SOURCE } from "../screens/github-pane/github";
import { SNAPSHOT_KEY as KIMI_SNAPSHOT_KEY, SOURCE as KIMI_SOURCE } from "../screens/kimi-pane/kimi";
import {
  BINDING_KEY as RACE_BINDING_KEY,
  SOURCE as RACE_SOURCE,
} from "../screens/race-pane/race";
import { SOURCE as UPTIME_SOURCE } from "../screens/uptime-pane/uptime";
import { BINDING_KEY, SNAPSHOT_KEY, SOURCE } from "../screens/waste-pane/waste";
import type { BindingDTO, PaneReadDTO } from "../store/protocol";

// The ranked region's demo world (#245) — a bound waste question whose
// collection is *tomorrow at the address*, so `?demo` photographs an
// answered, imminent "Trash Tonight" pane, and (#119) a bound `f1` race
// question twelve days out, the `distant` state that pane holds for most of
// the year.
//
// **The region is identical in both modes.** `NowScreen` swaps only these
// inputs for the store's; there is no demo-only rendering of the region, and
// there must never be one — the whole point of `QuestionInputs` being a
// plain value is that a fixture can drive the real shell rather than a
// parallel copy of it that drifts from it. What `?demo` shows is what ships.
//
// This one world now feeds BOTH surfaces (ADR-0017, #311): `NowScreen`'s
// aside filters it to the `"now"` questions below (waste answered/imminent,
// race answered/distant — one non-dormant and one quiet reading, so the
// capture proves both), and `StatusScreen` filters the same object to the
// `"status"` infra questions. One of those four still ignores
// `QuestionInputs` entirely — `reachability`, whose
// `screens/questions/placeholder.tsx` factory answers
// `bound-but-unacquired` unconditionally, because nothing polls behind it
// yet — so there is nothing to add here for it until #316 gives it a real
// source to read.
//
// **`kimi-balance/v1` (#313) is the first exception.** `kimiRead` below
// gives the Status capture its first poller-backed, non-gap pane: a modest
// "near" reading (the ADR's own worked example, `$4.10`) with a genuinely
// negative `cash_balance`, so the capture also exercises the voucher/cash
// split without needing a second, exhausted-balance world to prove it.
//
// **`github-hummingbird/v1` (#314) is the second.** `githubRead` below gives
// the Status capture five workflow rows, one per band the pane can produce
// (`live`/`imminent`/`near`/`distant`/`dormant`) — the collapsed-stack case
// the brief's acceptance line calls out ("this slice is the one that makes
// the region long"), so the 768px capture actually has five rows to prove
// readable rather than the one gap pane a fixture with no rows would leave
// it with.
//
// **`uptime/v1` (#315) is the third.** `uptimeRead` below gives the Status
// capture three service rows — `authority`, `web`, `runner` — all in quiet
// agreement, the honest steady state (see its own docstring). Between the
// three, the Status capture holds **nine poller-backed panes** (1 + 5 + 3)
// plus the single `reachability` gap pane.

/** The address the fixture's collection happens at. Fixed rather than the
 * device's own zone: a fixture whose answer changed with where the reviewer
 * is sitting would not be a fixture. */
const ZONE = "America/Los_Angeles";

function civilDateInZone(nowMs: number, dayOffset: number): string {
  const formatted = new Intl.DateTimeFormat("en-CA", {
    timeZone: ZONE,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(new Date(nowMs + dayOffset * 86_400_000));
  return formatted;
}

const boundBinding: BindingDTO = {
  key: BINDING_KEY,
  known: true,
  pending: false,
  value: { state: "text", text: "https://example.gov/waste/collection-day" },
};

function wasteRead(nowMs: number): PaneReadDTO {
  // Tomorrow at the address, on its scheduled day — an ordinary week, which
  // is the state worth photographing: the holiday variant is a change to the
  // words, not to the layout.
  const collectedOn = civilDateInZone(nowMs, 1);
  return {
    source: SOURCE,
    snapshots: [
      {
        key: SNAPSHOT_KEY,
        fetchedAtMs: nowMs - 40 * 60_000,
        envelope: {
          kind: "ok",
          schema: SOURCE,
          polledEveryMs: 86_400_000,
          body: JSON.stringify({
            zone: ZONE,
            scheduled: collectedOn,
            collected_on: collectedOn,
            streams: ["trash", "recycling", "yard"],
          }),
        },
        // Forty minutes old against a daily cadence: comfortably fresh, so
        // the tile shows no staleness line.
        freshness: { kind: "age", ageMs: 40 * 60_000, declaredCadenceMs: 86_400_000 },
      },
    ],
    liveAlerts: [],
  };
}

const boundRaceBinding: BindingDTO = {
  key: RACE_BINDING_KEY,
  known: true,
  pending: false,
  value: { state: "text", text: "f1" },
};

/** One followed series with a race twelve days out and its ladder ahead of
 * it — the `distant` state, which is what the pane looks like for most of the
 * year and therefore the honest thing to photograph. Anchored to `nowMs`
 * rather than to fixed instants so the capture never quietly goes off-season.
 */
function raceRead(nowMs: number): PaneReadDTO {
  // Snapped to the top of the hour: a fixture's own rendering should read
  // like a race time, not like whatever minute the capture ran at.
  const raceAtMs = Math.floor((nowMs + 12 * 86_400_000) / 3_600_000) * 3_600_000;
  return {
    source: RACE_SOURCE,
    snapshots: [
      {
        key: "f1",
        fetchedAtMs: nowMs - 40 * 60_000,
        envelope: {
          kind: "ok",
          schema: RACE_SOURCE,
          polledEveryMs: 6 * 60 * 60 * 1000,
          body: JSON.stringify({
            events: [
              {
                name: "Monaco Grand Prix",
                locality: "Monte Carlo",
                starts_at_ms: raceAtMs,
                sessions: [
                  { kind: "practice", label: "Practice 1", starts_at_ms: raceAtMs - 2 * 86_400_000 },
                  { kind: "qualifying", label: "Qualifying", starts_at_ms: raceAtMs - 86_400_000 },
                ],
              },
            ],
          }),
        },
        freshness: { kind: "age", ageMs: 40 * 60_000, declaredCadenceMs: 6 * 60 * 60 * 1000 },
      },
    ],
    // No live race-start alert: the loud state is one poll every fortnight,
    // and a fixture that showed it permanently would misrepresent the pane.
    liveAlerts: [],
  };
}

/** `$4.10` available, `$5.10` voucher, `-$1.00` cash — the ADR's own worked
 * example (decision 5) plus a genuinely negative cash position, so the
 * Status capture shows both the "near" band's wording and the voucher/cash
 * split in one pane, comfortably fresh against the 6h cadence. */
function kimiRead(nowMs: number): PaneReadDTO {
  return {
    source: KIMI_SOURCE,
    snapshots: [
      {
        key: KIMI_SNAPSHOT_KEY,
        fetchedAtMs: nowMs - 40 * 60_000,
        envelope: {
          kind: "ok",
          schema: KIMI_SOURCE,
          polledEveryMs: 21_600_000,
          body: JSON.stringify({
            available_balance: 4.1,
            voucher_balance: 5.1,
            cash_balance: -1.0,
          }),
        },
        freshness: { kind: "age", ageMs: 40 * 60_000, declaredCadenceMs: 21_600_000 },
      },
    ],
    liveAlerts: [],
  };
}

/** One row per band `githubBand` can produce, keyed by a real workflow file
 * name from `.github/workflows/` — the collapsed-stack case the brief's
 * acceptance line names ("this slice is the one that makes the region
 * long"). Every row is comfortably fresh against the 30h stale line (#371
 * review round 1's blocker 3 is about a *stale* answer, not this one), so
 * the capture shows the bands doing the work, not the staleness escalation
 * on top of them. */
function githubRead(nowMs: number): PaneReadDTO {
  function workflowSnapshot(
    key: string,
    displayName: string,
    body: {
      declaredCadenceMs: number | null;
      lastRunConclusion: string | null;
      lastRunEvent: string | null;
      lastRunAtMs: number | null;
      lastScheduledSuccessAtMs: number | null;
    },
  ) {
    return {
      key,
      fetchedAtMs: nowMs - 40 * 60_000,
      envelope: {
        kind: "ok" as const,
        schema: GITHUB_SOURCE,
        polledEveryMs: 86_400_000,
        body: JSON.stringify({
          display_name: displayName,
          declared_cadence_ms: body.declaredCadenceMs,
          last_run_conclusion: body.lastRunConclusion,
          last_run_event: body.lastRunEvent,
          last_run_at_ms: body.lastRunAtMs,
          last_scheduled_success_at_ms: body.lastScheduledSuccessAtMs,
        }),
      },
      // Forty minutes old against the poller's own daily cadence — fresh,
      // same margin `wasteRead`/`kimiRead` use.
      freshness: { kind: "age" as const, ageMs: 40 * 60_000, declaredCadenceMs: 86_400_000 },
    };
  }

  const fifteenMin = 15 * 60 * 1000;

  return {
    source: GITHUB_SOURCE,
    snapshots: [
      // `live` — never run at all, the auto-disable tell.
      workflowSnapshot("race-alert-poll.yml", "race-alert-poll", {
        declaredCadenceMs: 6 * 60 * 60 * 1000,
        lastRunConclusion: null,
        lastRunEvent: null,
        lastRunAtMs: null,
        lastScheduledSuccessAtMs: null,
      }),
      // `imminent` — has run, but its last *scheduled* success is well past
      // three times its own declared cadence.
      workflowSnapshot("calendar-poll.yml", "calendar-poll", {
        declaredCadenceMs: fifteenMin,
        lastRunConclusion: "success",
        lastRunEvent: "schedule",
        lastRunAtMs: nowMs - 90 * 60_000,
        lastScheduledSuccessAtMs: nowMs - 90 * 60_000,
      }),
      // `near` — a single recent failure, still on cadence otherwise.
      workflowSnapshot("graph-mail-poll.yml", "graph-mail-poll", {
        declaredCadenceMs: fifteenMin,
        lastRunConclusion: "failure",
        lastRunEvent: "schedule",
        lastRunAtMs: nowMs - 5 * 60_000,
        lastScheduledSuccessAtMs: nowMs - 20 * 60_000,
      }),
      // `distant` — a cron shape (weekly, day-of-week) the hand-rolled
      // parser correctly refuses to guess a cadence for, so the pane says
      // "cadence unreadable" rather than trusting the fall-through to
      // "healthy" (#371 review round 1's blocker 1).
      workflowSnapshot("graph-calendar-poll.yml", "graph-calendar-poll", {
        declaredCadenceMs: null,
        lastRunConclusion: "success",
        lastRunEvent: "schedule",
        lastRunAtMs: nowMs - 6 * 60 * 60 * 1000,
        lastScheduledSuccessAtMs: nowMs - 6 * 60 * 60 * 1000,
      }),
      // `dormant` — healthy, on cadence: the state most workflows sit in
      // most of the time, and the one the collapsed stack mostly renders.
      workflowSnapshot("gmail-poll.yml", "gmail-poll", {
        declaredCadenceMs: fifteenMin,
        lastRunConclusion: "success",
        lastRunEvent: "schedule",
        lastRunAtMs: nowMs - 5 * 60_000,
        lastScheduledSuccessAtMs: nowMs - 5 * 60_000,
      }),
    ],
    liveAlerts: [],
  };
}

/** One row per the three corrected `uptime/v1` services (ADR-0017 decision
 * 4 over #310/#315's finding) — `authority` and `runner` both healthy at
 * their declared refusal (401), `web` healthy at its declared 200. All
 * `dormant`/agreement: the demo world's honest steady state, since
 * `githubRead` already carries this region's non-dormant rows. */
function uptimeRead(nowMs: number): PaneReadDTO {
  function serviceSnapshot(key: string, expectStatus: number) {
    return {
      key,
      fetchedAtMs: nowMs - 5 * 60_000,
      envelope: {
        kind: "ok" as const,
        schema: UPTIME_SOURCE,
        polledEveryMs: 3_600_000,
        body: JSON.stringify({
          expected: "on",
          expect_status: expectStatus,
          observed_status: expectStatus,
          error: null,
        }),
      },
      freshness: { kind: "age" as const, ageMs: 5 * 60_000, declaredCadenceMs: 3_600_000 },
    };
  }

  return {
    source: UPTIME_SOURCE,
    snapshots: [
      serviceSnapshot("authority", 401),
      serviceSnapshot("web", 200),
      serviceSnapshot("runner", 401),
    ],
    liveAlerts: [],
  };
}

/** The demo world's answer to `QuestionInputs`, minus the clock the region
 * supplies itself. */
export function demoQuestionInputs(nowMs: number): Omit<QuestionInputs, "nowMs"> {
  return {
    bindings: [boundBinding, boundRaceBinding],
    paneReads: {
      [SOURCE]: wasteRead(nowMs),
      [RACE_SOURCE]: raceRead(nowMs),
      [KIMI_SOURCE]: kimiRead(nowMs),
      [GITHUB_SOURCE]: githubRead(nowMs),
      [UPTIME_SOURCE]: uptimeRead(nowMs),
    },
    // The demo world mounts no calendar credential and no items — `?demo`
    // photographs the snapshot-lane panes; the weekend pane's own demo state (a
    // `not_read` calendar, since nothing here ever pushes a token) is the
    // honest "unbound" reading rather than a hand-authored merge.
    calendarReads: {},
    // No calendar credential is ever mounted in the demo world, so
    // `calendarConnected: false` is the honest fact — the weekend pane's own
    // demo state is "unbound", never a stale-looking "checking" spinner.
    calendarConnected: false,
    items: [],
  };
}
