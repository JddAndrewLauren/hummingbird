import { describe, expect, it } from "vitest";
import { datetimeInputValueFromDuration, durationFromDatetimeInputValue } from "./deadline-picker";

// A fixed local instant to anchor every case: 2026-08-15T09:30 local time.
// `Date` in a vitest (node) environment reads the host's local zone, so
// these cases are stated relative to `NOW_MS` rather than an absolute
// wall-clock string.
const NOW_MS = new Date(2026, 7, 15, 9, 30, 0).getTime();

describe("datetimeInputValueFromDuration", () => {
  it("resolves a within_next duration to a future local moment", () => {
    const twoHoursLater = new Date(2026, 7, 15, 11, 30, 0);
    expect(datetimeInputValueFromDuration("2h", "within_next", NOW_MS)).toBe(
      `${twoHoursLater.getFullYear()}-08-15T11:30`,
    );
  });

  it("resolves a within_last duration to a past local moment", () => {
    expect(datetimeInputValueFromDuration("3d", "within_last", NOW_MS)).toBe("2026-08-12T09:30");
  });

  it("is empty for an unparseable stored value", () => {
    expect(datetimeInputValueFromDuration("", "within_next", NOW_MS)).toBe("");
    expect(datetimeInputValueFromDuration("soon", "within_next", NOW_MS)).toBe("");
  });
});

describe("durationFromDatetimeInputValue", () => {
  it("computes a within_next duration from a future picked moment, in minutes", () => {
    expect(durationFromDatetimeInputValue("2026-08-15T11:30", "within_next", NOW_MS)).toBe("120m");
  });

  it("computes a within_last duration from a past picked moment, in minutes", () => {
    expect(durationFromDatetimeInputValue("2026-08-12T09:30", "within_last", NOW_MS)).toBe("4320m");
  });

  it("clamps a moment on the 'wrong' side of now to one minute rather than a non-positive duration", () => {
    // A past moment picked for within_next (which wants a future target).
    expect(durationFromDatetimeInputValue("2026-08-10T09:30", "within_next", NOW_MS)).toBe("1m");
    // A future moment picked for within_last (which wants a past target).
    expect(durationFromDatetimeInputValue("2026-08-20T09:30", "within_last", NOW_MS)).toBe("1m");
  });

  it("is undefined for a mid-edit, incomplete input value", () => {
    expect(durationFromDatetimeInputValue("", "within_next", NOW_MS)).toBeUndefined();
  });

  it("round-trips through datetimeInputValueFromDuration", () => {
    const displayed = datetimeInputValueFromDuration("90m", "within_next", NOW_MS);
    expect(durationFromDatetimeInputValue(displayed, "within_next", NOW_MS)).toBe("90m");
  });
});
