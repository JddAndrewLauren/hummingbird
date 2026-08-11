import { describe, expect, it } from "vitest";
import {
  addCivilDays,
  civilDaysBetween,
  civilTodayInZone,
  deviceCivilToday,
  isCivilDate,
  isKnownZone,
  weekdayInZone,
  zonedMidnightMs,
} from "./zoned-day";

// The address's civil day, not the device's — see this module's own header
// for why a bin collection is a day at a place rather than an instant.

describe("civilTodayInZone", () => {
  it("gives different civil days either side of a zone boundary at the same instant", () => {
    // 2026-08-11T03:00Z: still the 10th in Los Angeles, already the 11th in
    // London. The person in the hotel must be told their own street's day.
    const instant = Date.parse("2026-08-11T03:00:00Z");
    expect(civilTodayInZone(instant, "America/Los_Angeles")).toBe("2026-08-10");
    expect(civilTodayInZone(instant, "Europe/London")).toBe("2026-08-11");
  });

  it("answers null for a zone this runtime does not know", () => {
    expect(civilTodayInZone(0, "Mars/Olympus_Mons")).toBeNull();
  });
});

describe("zonedMidnightMs", () => {
  it("resolves midnight at the address, not at UTC", () => {
    // Los Angeles is UTC-7 in August, so its midnight is 07:00Z.
    expect(zonedMidnightMs("2026-08-10", "America/Los_Angeles")).toBe(
      Date.parse("2026-08-10T07:00:00Z"),
    );
    expect(zonedMidnightMs("2026-08-10", "UTC")).toBe(Date.parse("2026-08-10T00:00:00Z"));
  });

  it("lands on the right side of a DST transition", () => {
    // US DST ends 2026-11-01. The day before is UTC-7, the day itself and the
    // day after are UTC-8 — a single-pass offset correction guesses from the
    // wrong side of the change and lands an hour out.
    expect(zonedMidnightMs("2026-10-31", "America/Los_Angeles")).toBe(
      Date.parse("2026-10-31T07:00:00Z"),
    );
    expect(zonedMidnightMs("2026-11-01", "America/Los_Angeles")).toBe(
      Date.parse("2026-11-01T07:00:00Z"),
    );
    expect(zonedMidnightMs("2026-11-02", "America/Los_Angeles")).toBe(
      Date.parse("2026-11-02T08:00:00Z"),
    );
  });

  it("answers null for an unknown zone or a date that is not a whole day", () => {
    expect(zonedMidnightMs("2026-08-10", "Mars/Olympus_Mons")).toBeNull();
    expect(zonedMidnightMs("2026-8-10", "UTC")).toBeNull();
    expect(zonedMidnightMs("tomorrow", "UTC")).toBeNull();
  });
});

describe("civilDaysBetween", () => {
  it("counts whole days across a DST transition, where the instants are 23 or 25 hours apart", () => {
    expect(civilDaysBetween("2026-10-31", "2026-11-03")).toBe(3);
    expect(civilDaysBetween("2026-08-10", "2026-08-10")).toBe(0);
    expect(civilDaysBetween("2026-08-11", "2026-08-10")).toBe(-1);
  });

  it("answers null for anything that is not a whole day", () => {
    expect(civilDaysBetween("2026-08-10", "nope")).toBeNull();
  });
});

describe("weekdayInZone", () => {
  it("names the day at the address", () => {
    expect(weekdayInZone("2026-08-10", "America/Los_Angeles")).toBe("Monday");
    expect(weekdayInZone("2026-08-11", "UTC")).toBe("Tuesday");
    expect(weekdayInZone("2026-08-10", "Mars/Olympus_Mons")).toBeNull();
  });
});

describe("the guards", () => {
  it("recognise a whole day and a real zone, and nothing else", () => {
    expect(isCivilDate("2026-08-10")).toBe(true);
    expect(isCivilDate("2026-08-10T00:00:00Z")).toBe(false);
    expect(isCivilDate(20260810)).toBe(false);
    expect(isKnownZone("Europe/London")).toBe(true);
    expect(isKnownZone("Nowhere/Nothing")).toBe(false);
  });
});

describe("addCivilDays", () => {
  it("walks the calendar, so an exclusive end minus one day is the last day", () => {
    // #121's own use: a provider's all-day end is local midnight AFTER the
    // last day, and the last day is that civil date minus one CIVIL day —
    // never `endMs - 86_400_000`, which lands a day out whenever the
    // boundary is not midnight in the zone doing the subtraction.
    expect(addCivilDays("2026-03-16", -1)).toBe("2026-03-15");
    expect(addCivilDays("2026-01-01", -1)).toBe("2025-12-31");
    expect(addCivilDays("2024-02-28", 1)).toBe("2024-02-29");
  });

  it("refuses anything that is not a civil date", () => {
    expect(addCivilDays("2026-3-1", 1)).toBeNull();
  });
});

describe("deviceCivilToday", () => {
  it("answers the device's own civil day, which is a different question from the address's", () => {
    // Deliberately asserted against the runtime's own formatter rather than
    // a fixed string: the point is that this reads the DEVICE zone, whatever
    // it is, where `civilTodayInZone` reads a named one.
    const nowMs = Date.parse("2026-03-01T12:00:00Z");
    const expected = new Intl.DateTimeFormat("en-CA", {
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
    }).format(new Date(nowMs));
    expect(deviceCivilToday(nowMs)).toBe(expected);
  });
});
