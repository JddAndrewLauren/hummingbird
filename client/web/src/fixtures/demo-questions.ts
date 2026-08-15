import type { QuestionInputs } from "../screens/questions/contract";
import {
  boundRaceBinding,
  boundWasteBinding,
  githubRead,
  GITHUB_SOURCE,
  kimiRead,
  KIMI_SOURCE,
  raceRead,
  RACE_SOURCE,
  uptimeRead,
  UPTIME_SOURCE,
  wasteRead,
  WASTE_SOURCE,
} from "./demo-pane-reads";

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
// `"status"` infra questions. Reachability reads the same sync snapshot
// shape as the real device, supplied below with a recent completed cycle.
//
// **`kimi-balance/v1` (#313) is the first exception.** `kimiRead` gives the
// Status capture its first poller-backed, non-gap pane: a modest "near"
// reading (the ADR's own worked example, `$4.10`) with a genuinely negative
// `cash_balance`, so the capture also exercises the voucher/cash split
// without needing a second, exhausted-balance world to prove it.
//
// **`github-hummingbird/v1` (#314) is the second.** `githubRead` gives the
// Status capture five workflow rows, one per band the pane can produce
// (`live`/`imminent`/`near`/`distant`/`dormant`) — the collapsed-stack case
// the brief's acceptance line calls out ("this slice is the one that makes
// the region long"), so the 768px capture actually has five rows to prove
// readable rather than the one gap pane a fixture with no rows would leave
// it with.
//
// **`uptime/v1` (#315) is the third.** `uptimeRead` gives the Status capture
// three service rows — `authority`, `web`, `runner` — all in quiet
// agreement, the honest steady state (see its own docstring). Between the
// three, the Status capture holds **nine poller-backed panes** (1 + 5 + 3)
// plus one device-local reachability answer.
//
// **The pane-read builders themselves live in `demo-pane-reads.ts` (#452),**
// shared with the board world's seed (`demo-task-state.ts`) — this module's
// own header's rule ("one input path, not a parallel copy of it that drifts
// from it") applied to itself: the board world needed the same
// `PaneReadDTO`s, and a second hand-authored copy would be exactly the drift
// this file exists to prevent everywhere else.

/** The demo world's answer to `QuestionInputs`, minus the clock the region
 * supplies itself. */
export function demoQuestionInputs(nowMs: number): Omit<QuestionInputs, "nowMs"> {
  return {
    sync: {
      latestOutcome: {
        kind: "completed",
        retryAfterMs: null,
        activeItemCount: 12,
        wasFullSweep: false,
        deadLettered: 0,
      },
      latestInformativeAtMs: nowMs - 60_000,
      lastSuccessfulAtMs: nowMs - 60_000,
    },
    bindings: [boundWasteBinding, boundRaceBinding],
    paneReads: {
      [WASTE_SOURCE]: wasteRead(nowMs),
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
