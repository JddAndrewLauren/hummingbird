import type { QuestionInputs } from "../screens/questions/contract";
import {
  BINDING_KEY as RACE_BINDING_KEY,
  SOURCE as RACE_SOURCE,
} from "../screens/race-pane/race";
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
// four `"status"` infra questions. Those four ignore `QuestionInputs`
// entirely — `screens/questions/placeholder.ts`'s factory answers
// `bound-but-unacquired` unconditionally, because no poller exists behind
// any of them yet — so there is nothing to add here for Status specifically
// until #313-#316 give each one a real source to read: the demo capture
// photographs exactly the four honest gap panes the Acceptance section's
// manual check asks for, not a fabricated "healthy" or "diverging" reading
// this slice has no data to back.



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

/** The demo world's answer to `QuestionInputs`, minus the clock the region
 * supplies itself. */
export function demoQuestionInputs(nowMs: number): Omit<QuestionInputs, "nowMs"> {
  return {
    bindings: [boundBinding, boundRaceBinding],
    paneReads: { [SOURCE]: wasteRead(nowMs), [RACE_SOURCE]: raceRead(nowMs) },
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
