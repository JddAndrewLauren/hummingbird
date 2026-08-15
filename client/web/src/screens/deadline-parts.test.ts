import { describe, expect, it } from "vitest";
import { joinDeadline, splitDeadline } from "./deadline-parts";

describe("splitDeadline", () => {
  it("reads a whole-day deadline as a date with no time", () => {
    expect(splitDeadline("2026-09-01")).toEqual({ date: "2026-09-01", time: null });
  });

  it("splits a date-time into its two controls", () => {
    expect(splitDeadline("2026-09-01T09:30")).toEqual({ date: "2026-09-01", time: "09:30" });
  });

  it("reads an empty deadline as an empty date", () => {
    expect(splitDeadline("")).toEqual({ date: "", time: null });
  });

  it("passes a shape it does not recognise straight through", () => {
    // An item captured before the picker existed, or by a skill, may carry
    // free text here. Emptying the field on load would delete it the moment
    // anything else on the form was saved.
    for (const raw of ["next tuesday", "2026-09-01T09:30:00", "2026-9-1", "09:30"]) {
      expect(splitDeadline(raw)).toEqual({ date: raw, time: null });
    }
  });
});

describe("joinDeadline", () => {
  it("round-trips both shapes", () => {
    for (const value of ["2026-09-01", "2026-09-01T09:30", ""]) {
      const parts = splitDeadline(value);
      expect(joinDeadline(parts.date, parts.time)).toBe(value);
    }
  });

  it("drops the time when the date is cleared — a time with no day is not a deadline", () => {
    expect(joinDeadline("", "09:30")).toBe("");
  });

  it("treats an empty time the same as no time", () => {
    expect(joinDeadline("2026-09-01", "")).toBe("2026-09-01");
    expect(joinDeadline("2026-09-01", null)).toBe("2026-09-01");
  });

  it("refuses to build a date-time on a date it does not recognise", () => {
    // Appending `T09:30` to free text would turn one unsendable value into a
    // stranger one; the raw text stays as it is.
    expect(joinDeadline("next tuesday", "09:30")).toBe("next tuesday");
  });

  it("does not recognise a merely date-shaped legacy value", () => {
    // The near-miss: ten characters with hyphens in the right two places is
    // not a date. Recognition is digit-by-digit, so these stay as they are
    // rather than being rewritten into a date-time that was never a deadline.
    for (const raw of ["abcd-ef-gh", "2026-0x-01", "q3-planning"]) {
      expect(joinDeadline(raw, "09:30")).toBe(raw);
    }
  });
});
