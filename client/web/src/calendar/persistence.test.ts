import { describe, expect, it } from "vitest";
import {
  readConnected,
  readSelectedCalendarIds,
  type StorageLike,
  writeConnected,
  writeSelectedCalendarIds,
} from "./persistence";

function fakeStorage(): StorageLike {
  const map = new Map<string, string>();
  return {
    getItem: (key) => map.get(key) ?? null,
    setItem: (key, value) => {
      map.set(key, value);
    },
    removeItem: (key) => {
      map.delete(key);
    },
  };
}

describe("connected flag", () => {
  it("defaults to false (a never-opted-in device)", () => {
    expect(readConnected(fakeStorage())).toBe(false);
  });

  it("round-trips true", () => {
    const storage = fakeStorage();
    writeConnected(storage, true);
    expect(readConnected(storage)).toBe(true);
  });

  it("clears the flag entirely rather than storing 'false'", () => {
    const storage = fakeStorage();
    writeConnected(storage, true);
    writeConnected(storage, false);
    expect(readConnected(storage)).toBe(false);
    expect(storage.getItem("hb.calendar.connected")).toBeNull();
  });
});

describe("selected calendar ids", () => {
  it("defaults to empty", () => {
    expect(readSelectedCalendarIds(fakeStorage())).toEqual([]);
  });

  it("round-trips a selection", () => {
    const storage = fakeStorage();
    writeSelectedCalendarIds(storage, ["a", "b"]);
    expect(readSelectedCalendarIds(storage)).toEqual(["a", "b"]);
  });

  it("treats malformed stored JSON as empty rather than throwing", () => {
    const storage = fakeStorage();
    storage.setItem("hb.calendar.selectedCalendarIds", "{not json");
    expect(readSelectedCalendarIds(storage)).toEqual([]);
  });

  it("filters out non-string entries from a tampered value", () => {
    const storage = fakeStorage();
    storage.setItem("hb.calendar.selectedCalendarIds", JSON.stringify(["a", 1, null]));
    expect(readSelectedCalendarIds(storage)).toEqual(["a"]);
  });
});
