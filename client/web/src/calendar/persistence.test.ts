import { describe, expect, it } from "vitest";
import { clearLocalCalendarPrefs, readLocalCalendarPrefs, type StorageLike } from "./persistence";

function fakeStorage(entries: Record<string, string> = {}): StorageLike {
  const map = new Map<string, string>(Object.entries(entries));
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

describe("the old local calendar choices (ADR-0040 decision 8's migration input)", () => {
  it("are absent on a browser that never connected", () => {
    expect(readLocalCalendarPrefs(fakeStorage())).toBeNull();
  });

  it("read back the flag and the ticked ids", () => {
    const storage = fakeStorage({
      "hb.calendar.connected": "true",
      "hb.calendar.selectedCalendarIds": JSON.stringify(["a", "b"]),
    });
    expect(readLocalCalendarPrefs(storage)).toEqual({ connected: true, selectedIds: ["a", "b"] });
  });

  it("read ids alone as not connected", () => {
    const storage = fakeStorage({ "hb.calendar.selectedCalendarIds": JSON.stringify(["a"]) });
    expect(readLocalCalendarPrefs(storage)).toEqual({ connected: false, selectedIds: ["a"] });
  });

  it("treat malformed or tampered ids as none rather than throwing", () => {
    expect(
      readLocalCalendarPrefs(
        fakeStorage({ "hb.calendar.connected": "true", "hb.calendar.selectedCalendarIds": "{not json" }),
      ),
    ).toEqual({ connected: true, selectedIds: [] });
    expect(
      readLocalCalendarPrefs(
        fakeStorage({ "hb.calendar.selectedCalendarIds": JSON.stringify(["a", 1, null]) }),
      ),
    ).toEqual({ connected: false, selectedIds: ["a"] });
  });

  it("are gone once cleared", () => {
    const storage = fakeStorage({
      "hb.calendar.connected": "true",
      "hb.calendar.selectedCalendarIds": JSON.stringify(["a"]),
    });
    clearLocalCalendarPrefs(storage);
    expect(readLocalCalendarPrefs(storage)).toBeNull();
    expect(storage.getItem("hb.calendar.connected")).toBeNull();
    expect(storage.getItem("hb.calendar.selectedCalendarIds")).toBeNull();
  });
});
