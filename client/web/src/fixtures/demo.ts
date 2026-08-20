// The board world's seam between real data and its seeded `TaskState`.
//
// Gated twice, deliberately. `import.meta.env.DEV` is substituted with the
// literal `false` at build time, so the production bundle contains
// `if (false && …)` — demo mode cannot be switched on by a query string in
// production even if someone tries, and Rollup drops the fixture with the
// dead branch. The `?demo` check is what makes it opt-in during development,
// so `pnpm dev` still shows the honest empty states by default.
//
// The kit world's own accessor, `demoData()`, moved to `fixtures/demo-data.ts`
// in #457 — colocated with `DEMO_DATA`, and called directly by `AlertsScreen`
// (its only caller since #624 deleted `RoutesScreen`) rather than threaded
// through `App.tsx` as a `demo` prop.
// This module now holds only the board world's two accessors.

import { buildDemoCalendarState } from "./demo-calendar";
import { demoMode } from "./demo-mode";
import { buildDemoTaskState } from "./demo-task-state";
import type { CalendarState, TaskState } from "../store/store";

/** The board world's seeded `TaskState` (#420), or `null` for every other
 * mode. Behind the same double gate `demoData` is: substituted to
 * `if (false && …)` in a production build, so Rollup drops the whole fixture
 * with the dead branch and no fixture item can reach a real device.
 *
 * That claim held for `DEMO_DATA` and did NOT hold for this one until the
 * fixture was rewritten to build inside a function — a dead branch only drops
 * a module Rollup can prove is side-effect-free at the top level, and the
 * first cut read the clock and called a helper while constructing its seeds.
 * `pnpm assert-no-fixtures` is what stops that returning; it runs in CI right
 * after the build, because this is a claim about the artifact and only the
 * artifact can answer it.
 *
 * Deliberately mutually exclusive with `demo-data.ts`'s `demoData()` —
 * `?demo=board` (and every spelling but `?demo=kit`) returns a state here and
 * `null` there, `demoMode()`'s whole mechanism. `App.tsx`'s
 * `task = demoTask ?? liveTask` is what makes every screen but Alerts (#457,
 * its own dev-gated `demoData()` reader) take its real render path against
 * this seed. */
export function demoTaskState(): TaskState | null {
  if (!import.meta.env.DEV) {
    return null;
  }
  return demoMode(window.location.search) === "board" ? buildDemoTaskState() : null;
}

/** The board world's seeded calendar (#452, piece 4), or `null` everywhere
 * else — same double gate and the same reason `demoTaskState` is deliberately
 * `TaskState`-shaped rather than a store override: `CalendarState` feeds
 * `useCalendarWiring`'s real effects, so `App.tsx` injects this at the prop
 * boundary (`SettingsScreen`'s `calendar` prop only) and leaves the hook
 * itself reading the live store slice. See `demo-calendar.ts`'s header for
 * the whole argument. */
export function demoCalendar(): CalendarState | null {
  if (!import.meta.env.DEV) {
    return null;
  }
  return demoMode(window.location.search) === "board" ? buildDemoCalendarState() : null;
}
