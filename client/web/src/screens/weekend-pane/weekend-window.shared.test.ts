import { describe, expect, it } from "vitest";

import { weekendWindowFromCore, weekendZoneQueriesFromCore } from "../../decisions/seam";
import { resolveZoneFacts } from "../questions/zone-bridge";
import { weekendWindow } from "./weekend";

// The cross-host pin `weekend.ts`'s module header promises: `weekendWindow`
// stays local TS (the `describe`-collection-order trap that section
// explains), but `weekend.rs`'s `weekend_window` is still the canonical
// definition. This file is what makes that true rather than aspirational —
// on `shared-fixtures.test.ts`'s own precedent (this host's `Intl` resolver
// reproduces the core's own answer), asserted directly against the core
// through the seam rather than a hand-typed fixture table, since the whole
// point here is "these two computations must never diverge", not "this
// scenario's numbers are frozen".

function at(y: number, m: number, d: number, h = 0, min = 0): number {
  return new Date(y, m - 1, d, h, min).getTime();
}

describe("weekendWindow, pinned against the core's weekend_window", () => {
  const scenarios: [string, number][] = [
    ["a Friday afternoon, ahead of the window", at(2026, 8, 14, 16, 0)],
    ["Friday at 17:00 sharp, the window's own open", at(2026, 8, 14, 17, 0)],
    ["a weekday, the coming weekend", at(2026, 8, 10, 9, 0)],
    ["Sunday 19:59, still under way", at(2026, 8, 16, 19, 59)],
    ["Sunday 20:01, rolled to the following weekend", at(2026, 8, 16, 20, 1)],
    ["mid-Saturday, under way", at(2026, 8, 15, 12, 0)],
  ];

  it.each(scenarios)("agrees with the core — %s", (_name, nowMs) => {
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
});
