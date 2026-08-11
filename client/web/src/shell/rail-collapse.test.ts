import { describe, expect, it } from "vitest";
import { readRailCollapsed, writeRailCollapsed, type StorageLike } from "./rail-collapse";

function fakeStorage(initial: Record<string, string> = {}): StorageLike & { data: Record<string, string> } {
  const data = { ...initial };
  return {
    data,
    getItem: (key) => (key in data ? data[key]! : null),
    setItem: (key, value) => {
      data[key] = value;
    },
    removeItem: (key) => {
      delete data[key];
    },
  };
}

describe("rail-collapse", () => {
  it("defaults to expanded — with no storage, an empty one, or garbage in the key", () => {
    expect(readRailCollapsed(undefined)).toBe(false);
    expect(readRailCollapsed(fakeStorage())).toBe(false);
    expect(readRailCollapsed(fakeStorage({ "hb.shell.rail-collapsed": "banana" }))).toBe(false);
  });

  it("round-trips a collapse, and expanding removes the key rather than storing a second default", () => {
    const storage = fakeStorage();
    writeRailCollapsed(storage, true);
    expect(readRailCollapsed(storage)).toBe(true);
    writeRailCollapsed(storage, false);
    expect(readRailCollapsed(storage)).toBe(false);
    expect(Object.keys(storage.data)).toHaveLength(0);
  });

  it("tolerates a storage that throws — the preference just doesn't persist", () => {
    const broken: StorageLike = {
      getItem: () => {
        throw new Error("private mode");
      },
      setItem: () => {
        throw new Error("private mode");
      },
      removeItem: () => {
        throw new Error("private mode");
      },
    };
    expect(readRailCollapsed(broken)).toBe(false);
    expect(() => writeRailCollapsed(broken, true)).not.toThrow();
  });
});
