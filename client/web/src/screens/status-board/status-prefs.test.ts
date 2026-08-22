import { describe, expect, it } from "vitest";
import type { StorageLike } from "../storage";
import { readExpandedKey, writeExpandedKey } from "./status-prefs";

function stub(
  seed: Record<string, string> = {},
): StorageLike & { keys: () => string[] } {
  const map = new Map(Object.entries(seed));
  return {
    getItem: (key) => map.get(key) ?? null,
    setItem: (key, value) => void map.set(key, value),
    removeItem: (key) => void map.delete(key),
    keys: () => [...map.keys()],
  };
}

const THROWS: StorageLike = {
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

describe("the Status board's open-tile preference", () => {
  it("round-trips the open tile", () => {
    const storage = stub();
    writeExpandedKey(storage, "uptime:runner");
    expect(readExpandedKey(storage)).toBe("uptime:runner");
  });

  it("encodes 'nothing open' as the key's absence, never as a stored value", () => {
    const storage = stub();
    writeExpandedKey(storage, "uptime:runner");
    writeExpandedKey(storage, null);
    expect(storage.keys()).toEqual([]);
    expect(readExpandedKey(storage)).toBeNull();
  });

  it("reads nothing open when there is no storage at all", () => {
    expect(readExpandedKey(undefined)).toBeNull();
  });

  // A preference that cannot persist still applies for the session; it must
  // never take the screen down with it (private-mode Safari throws).
  it("degrades rather than throwing when storage itself throws", () => {
    expect(readExpandedKey(THROWS)).toBeNull();
    expect(() => writeExpandedKey(THROWS, "uptime:runner")).not.toThrow();
    expect(() => writeExpandedKey(THROWS, null)).not.toThrow();
  });

  it("keeps its key out of the questions' collapse namespace", () => {
    const storage = stub();
    writeExpandedKey(storage, "uptime:runner");
    expect(storage.keys()).toEqual(["hb.status.expanded"]);
  });
});
