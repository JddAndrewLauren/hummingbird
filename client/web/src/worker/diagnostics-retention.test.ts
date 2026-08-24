import { describe, expect, it } from "vitest";
import { planRetention, type RetentionRecord } from "./diagnostics-retention";

const OPTIONS = { retentionMs: 72 * 60 * 60 * 1000, maxBytes: 10 * 1024 * 1024 };

function record(key: number, wallClockMs: number, byteLength = 100): RetentionRecord {
  return { key, wallClockMs, byteLength };
}

describe("planRetention", () => {
  it("evicts nothing when every record is within both bounds", () => {
    const records = [record(1, 1_000), record(2, 2_000)];
    const plan = planRetention(records, 10_000, OPTIONS);
    expect(plan.evictKeys).toEqual([]);
    expect(plan.droppedCount).toBe(0);
  });

  it("evicts a record older than 72 hours by the injected clock, oldest first", () => {
    const dayMs = 24 * 60 * 60 * 1000;
    const nowMs = 10 * dayMs;
    const records = [
      record(1, nowMs - 4 * dayMs), // too old
      record(2, nowMs - 3 * dayMs, 100), // exactly at 72h boundary minus a bit
      record(3, nowMs - 1 * dayMs), // fresh
    ];
    const plan = planRetention(records, nowMs, OPTIONS);
    expect(plan.evictKeys).toEqual([1]);
    expect(plan.droppedCount).toBe(1);
  });

  it("never age-evicts a record exactly at the 72h cutoff or newer", () => {
    const nowMs = 100_000_000;
    const records = [record(1, nowMs - OPTIONS.retentionMs)];
    const plan = planRetention(records, nowMs, OPTIONS);
    expect(plan.evictKeys).toEqual([]);
  });

  it("evicts the oldest-by-key records first once the byte bound is crossed", () => {
    const nowMs = 1_000;
    // Three records of 4 MiB each: total 12 MiB > 10 MiB cap.
    const big = 4 * 1024 * 1024;
    const records = [record(1, nowMs, big), record(2, nowMs, big), record(3, nowMs, big)];
    const plan = planRetention(records, nowMs, OPTIONS);
    // Dropping the single oldest (key 1) leaves 8 MiB, under the cap.
    expect(plan.evictKeys).toEqual([1]);
    expect(plan.droppedCount).toBe(1);
  });

  it("keeps evicting oldest-first until the byte bound is satisfied", () => {
    const nowMs = 1_000;
    const big = 4 * 1024 * 1024;
    const records = [
      record(1, nowMs, big),
      record(2, nowMs, big),
      record(3, nowMs, big),
      record(4, nowMs, big),
    ];
    // 16 MiB total; must drop keys 1 and 2 to reach 8 MiB, under the cap.
    const plan = planRetention(records, nowMs, OPTIONS);
    expect(plan.evictKeys).toEqual([1, 2]);
  });

  it("applies age eviction before size eviction, over the same record set", () => {
    const dayMs = 24 * 60 * 60 * 1000;
    const nowMs = 10 * dayMs;
    const big = 6 * 1024 * 1024;
    const records = [
      record(1, nowMs - 4 * dayMs, big), // aged out regardless of size
      record(2, nowMs, big),
      record(3, nowMs, big),
    ];
    const plan = planRetention(records, nowMs, OPTIONS);
    // key 1 drops for age; that alone brings the surviving set (2,3) to
    // 12 MiB, still over the cap, so key 2 also drops for size.
    expect(plan.evictKeys).toEqual([1, 2]);
    expect(plan.droppedCount).toBe(2);
  });

  it("never double-counts a record evicted for age when scanning for size", () => {
    const dayMs = 24 * 60 * 60 * 1000;
    const nowMs = 10 * dayMs;
    const records = [record(1, nowMs - 4 * dayMs, 999_999_999)];
    const plan = planRetention(records, nowMs, OPTIONS);
    expect(plan.evictKeys).toEqual([1]);
    expect(plan.droppedCount).toBe(1);
  });
});
