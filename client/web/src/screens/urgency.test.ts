import { describe, expect, it } from "vitest";
import { computeUrgency, deadlineSortKey } from "./urgency";

// CONTEXT.md: "Urgency is a derived, time-varying property of items,
// computed by consumers at read time over the mirror. Never a stored
// class" — `computeUrgency` is that consumer-side function, over the raw
// `deadline` string and a caller-supplied clock. No `TaskItemDTO` field
// ever carries a stored urgency; see `store/protocol.ts`'s `TaskItemDTO`.

describe("deadlineSortKey", () => {
  it("resolves a day-only deadline to that day's end (23:59)", () => {
    expect(deadlineSortKey("2026-08-15")).toBe("2026-08-15T23:59");
  });

  it("returns a minute-precision deadline unchanged", () => {
    expect(deadlineSortKey("2026-08-15T09:30")).toBe("2026-08-15T09:30");
  });
});

describe("computeUrgency", () => {
  const NOW = new Date(2026, 7, 15, 12, 0).getTime(); // 2026-08-15 noon, local

  it("is 'calm' with no deadline at all", () => {
    expect(computeUrgency(null, NOW)).toBe("calm");
  });

  it("is 'overdue' once the deadline has passed", () => {
    expect(computeUrgency("2026-08-14", NOW)).toBe("overdue");
    expect(computeUrgency("2026-08-15T11:00", NOW)).toBe("overdue");
  });

  it("is 'now' within the near-term window", () => {
    expect(computeUrgency("2026-08-15T18:00", NOW)).toBe("now");
  });

  it("is 'soon' further out but still within days", () => {
    expect(computeUrgency("2026-08-17", NOW)).toBe("soon");
  });

  it("is 'calm' for a deadline far in the future", () => {
    expect(computeUrgency("2026-12-01", NOW)).toBe("calm");
  });

  it("a day-only deadline on today is still 'now' at noon, not 'overdue'", () => {
    // Resolves to 2026-08-15T23:59 — end of day, still ahead of NOW.
    expect(computeUrgency("2026-08-15", NOW)).toBe("now");
  });

  it("treats an unparseable deadline as 'calm' rather than throwing", () => {
    expect(computeUrgency("not-a-date", NOW)).toBe("calm");
  });
});
