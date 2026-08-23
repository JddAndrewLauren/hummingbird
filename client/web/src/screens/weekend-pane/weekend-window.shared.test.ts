import { describe, expect, it } from "vitest";

import {
  weekendBandFromCore,
  weekendWindowFromCore,
  weekendWithinBandFromCore,
  weekendZoneQueriesFromCore,
} from "../../decisions/seam";
import { resolveZoneFacts } from "../questions/zone-bridge";
import { weekendBand, weekendWindow, weekendWithinBand, type WeekendWindow } from "./weekend";

// The cross-host pin `weekend.ts`'s module header promises: `weekendWindow`/
// `weekendBand`/`weekendWithinBand` stay local TS (the `describe`-
// collection-order trap that section explains), but `weekend.rs`'s
// `weekend_window`/`weekend_band`/`weekend_within_band` are still the
// canonical definitions. This file is what makes that true rather than
// aspirational — on `shared-fixtures.test.ts`'s own precedent (this host's
// own computation reproduces the core's own answer), asserted directly
// against the core through the seam rather than a hand-typed fixture table,
// since the whole point here is "these two computations must never
// diverge", not "this scenario's numbers are frozen".

function at(y: number, m: number, d: number, h = 0, min = 0): number {
  return new Date(y, m - 1, d, h, min).getTime();
}

/** The local `WeekendWindow`'s wire shape, as `weekend_band_json`/
 * `weekend_within_band_json` read it — `days[].key` maps to `days[].date`,
 * and `entries` (present-but-empty here) carries nothing either side
 * reads for band/within-band. */
function toCoreWindow(window: WeekendWindow) {
  return {
    startMs: window.startMs,
    endMs: window.endMs,
    underWay: window.underWay,
    days: window.days.map((day) => ({ date: day.key, startMs: day.startMs, endMs: day.endMs })),
  };
}

describe("weekendWindow, pinned against the core's weekend_window", () => {
  const scenarios: [string, number][] = [
    ["a Friday afternoon, ahead of the window", at(2026, 8, 14, 16, 0)],
    ["Friday at 17:00 sharp, the window's own open", at(2026, 8, 14, 17, 0)],
    ["a weekday, the coming weekend", at(2026, 8, 10, 9, 0)],
    ["Sunday 19:59, still under way", at(2026, 8, 16, 19, 59)],
    ["Sunday 20:01, rolled to the following weekend", at(2026, 8, 16, 20, 1)],
    ["mid-Saturday, under way", at(2026, 8, 15, 12, 0)],
    // The shrink's own boundary, on both sides of it: the two windows have
    // to agree about *which* days are left, not only about the weekend's
    // start and end. Friday's last millisecond still holds three days;
    // Saturday's first holds two; Sunday morning holds one.
    ["Friday's last millisecond, three days left", at(2026, 8, 15) - 1],
    ["Saturday's first millisecond, Friday gone", at(2026, 8, 15)],
    ["Sunday morning, Sunday alone", at(2026, 8, 16, 9, 0)],
    // Band boundaries, exercised at instants distinct from the window
    // scenarios above so the pin also covers the imminent/near/dormant
    // edges rather than only the window's own start/rollover instants.
    ["48h before the window opens — the imminent boundary", at(2026, 8, 12, 17, 0)],
    ["96h before the window opens — the near boundary", at(2026, 8, 10, 17, 0)],
    ["a week before the window opens — dormant", at(2026, 8, 7, 9, 0)],
  ];

  it.each(scenarios)("agrees with the core on the window — %s", (_name, nowMs) => {
    const queries = weekendZoneQueriesFromCore(nowMs);
    const facts = resolveZoneFacts(queries);
    const core = weekendWindowFromCore(nowMs, facts);
    const local = weekendWindow(nowMs);

    expect(core).not.toBeNull();
    if (core === null) return;
    expect(local.startMs).toBe(core.startMs);
    expect(local.endMs).toBe(core.endMs);
    expect(local.underWay).toBe(core.underWay);
    expect(local.days.map((day) => day.key)).toEqual(core.days.map((day) => day.date));
    expect(local.days.map((day) => day.startMs)).toEqual(core.days.map((day) => day.startMs));
    expect(local.days.map((day) => day.endMs)).toEqual(core.days.map((day) => day.endMs));
  });

  it.each(scenarios)("agrees with the core on the band and withinBand — %s", (_name, nowMs) => {
    const local = weekendWindow(nowMs);
    const coreWindow = toCoreWindow(local);

    expect(weekendBand(local, nowMs)).toBe(weekendBandFromCore(coreWindow, nowMs));
    expect(weekendWithinBand(local)).toBe(weekendWithinBandFromCore(coreWindow));
  });
});
