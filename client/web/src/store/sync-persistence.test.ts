import { describe, expect, it } from "vitest";
import {
  advanceLastSuccessfulSyncAtMs,
  readLastSuccessfulSyncAtMs,
  type SyncStorageLike,
} from "./sync-persistence";

function fakeStorage(): SyncStorageLike {
  const values = new Map<string, string>();
  return {
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => {
      values.set(key, value);
    },
  };
}

describe("last successful sync persistence", () => {
  it("reads missing and malformed values as no history", () => {
    const storage = fakeStorage();
    expect(readLastSuccessfulSyncAtMs(storage)).toBeNull();
    storage.setItem("hb.sync.lastSuccessfulAtMs", "not json");
    expect(readLastSuccessfulSyncAtMs(storage)).toBeNull();
    storage.setItem("hb.sync.lastSuccessfulAtMs", JSON.stringify(-1));
    expect(readLastSuccessfulSyncAtMs(storage)).toBeNull();
  });

  it("round-trips a successful timestamp", () => {
    const storage = fakeStorage();
    expect(advanceLastSuccessfulSyncAtMs(storage, null, 12_345, 12_345)).toBe(12_345);
    expect(readLastSuccessfulSyncAtMs(storage)).toBe(12_345);
  });

  it("does not regress a newer success", () => {
    const storage = fakeStorage();
    advanceLastSuccessfulSyncAtMs(storage, null, 12_345, 12_345);
    expect(advanceLastSuccessfulSyncAtMs(storage, 12_345, 10_000, 13_000)).toBe(12_345);
    expect(readLastSuccessfulSyncAtMs(storage)).toBe(12_345);
  });

  it("does not let a stale view overwrite newer storage", () => {
    const storage = fakeStorage();
    advanceLastSuccessfulSyncAtMs(storage, null, 12_345, 12_345);
    expect(advanceLastSuccessfulSyncAtMs(storage, 8_000, 10_000, 13_000)).toBe(12_345);
    expect(readLastSuccessfulSyncAtMs(storage)).toBe(12_345);
  });

  it("recovers when a clock correction leaves the persisted success in the future", () => {
    const storage = fakeStorage();
    advanceLastSuccessfulSyncAtMs(storage, null, 20_000, 20_000);

    expect(advanceLastSuccessfulSyncAtMs(storage, 20_000, 10_000, 10_100)).toBe(10_000);
    expect(readLastSuccessfulSyncAtMs(storage)).toBe(10_000);
  });
});
