// The standing-question pane reads the board world's seed answers with —
// moved out of `demo-questions.ts` (#452) so the board world
// (`demo-task-state.ts`) could seed the SAME `PaneReadDTO`s the kit world's
// own fixture used to hand-author a second, drifting copy of. That module's
// own header stated the rule this file exists to keep: "one input path, not
// a parallel copy of it that drifts from it" — and #455, flipping the
// default from the kit world to the board world, deleted `demo-questions.ts`
// outright, leaving this module's one caller `demo-task-state.ts`.
//
// Every function here takes `nowMs` and returns a value — no clock read at
// module scope, no top-level literal built from one — for the same bundling
// reason `demo-task-state.ts`'s header documents at length: a fixture module
// Rollup cannot prove side-effect-free at the top level does not drop from
// the production bundle. Its caller is itself gated behind
// `import.meta.env.DEV`, so this module carries no gate of its own.

import { SOURCE as GITHUB_SOURCE } from "../screens/github-pane/github";
import { SNAPSHOT_KEY as KIMI_SNAPSHOT_KEY, SOURCE as KIMI_SOURCE } from "../screens/kimi-pane/kimi";
import {
  BINDING_KEY as RACE_BINDING_KEY,
  SOURCE as RACE_SOURCE,
} from "../screens/race-pane/race";
import { SOURCE as UPTIME_SOURCE } from "../screens/uptime-pane/uptime";
import { BINDING_KEY as WASTE_BINDING_KEY, SNAPSHOT_KEY, SOURCE as WASTE_SOURCE } from "../screens/waste-pane/waste";
import type { BindingDTO, PaneReadDTO } from "../store/protocol";

export {
  GITHUB_SOURCE,
  KIMI_SOURCE,
  RACE_SOURCE,
  UPTIME_SOURCE,
  WASTE_SOURCE,
  WASTE_BINDING_KEY,
  RACE_BINDING_KEY,
};

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

export const boundWasteBinding: BindingDTO = {
  key: WASTE_BINDING_KEY,
  known: true,
  pending: false,
  value: { state: "text", text: "https://example.gov/waste/collection-day" },
};

/** Tomorrow at the address, on its scheduled day — an ordinary week, which is
 * the state worth photographing: the holiday variant is a change to the
 * words, not to the layout. */
export function wasteRead(nowMs: number): PaneReadDTO {
  const collectedOn = civilDateInZone(nowMs, 1);
  return {
    source: WASTE_SOURCE,
    snapshots: [
      {
        key: SNAPSHOT_KEY,
        fetchedAtMs: nowMs - 40 * 60_000,
        envelope: {
          kind: "ok",
          schema: WASTE_SOURCE,
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

export const boundRaceBinding: BindingDTO = {
  key: RACE_BINDING_KEY,
  known: true,
  pending: false,
  value: { state: "text", text: "f1" },
};

/** One followed series with a race twelve days out and its ladder ahead of
 * it — the `distant` state, which is what the pane looks like for most of the
 * year and therefore the honest thing to photograph. Anchored to `nowMs`
 * rather than to fixed instants so the fixture never quietly goes
 * off-season. */
export function raceRead(nowMs: number): PaneReadDTO {
  // Snapped to the top of the hour: a fixture's own rendering should read
  // like a race time, not like whatever minute it was built at.
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
 * capture shows both the "near" band's wording and the voucher/cash split in
 * one pane, comfortably fresh against the 6h cadence. */
export function kimiRead(nowMs: number): PaneReadDTO {
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
 * name from `.github/workflows/` — the collapsed-stack case. Every row is
 * comfortably fresh against the 30h stale line, so the fixture shows the
 * bands doing the work, not the staleness escalation on top of them. */
export function githubRead(nowMs: number): PaneReadDTO {
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
      // "healthy".
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

/** One row per the three corrected `uptime/v1` services — `authority` and
 * `runner` both healthy at their declared refusal (401), `web` healthy at
 * its declared 200. All `dormant`/agreement: the honest steady state. */
export function uptimeRead(nowMs: number): PaneReadDTO {
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
