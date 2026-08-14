// The same five-test template `frontier-prefs.test.ts` and
// `shell/rail-collapse.test.ts` use: default, round-trip, key-removal-not-
// default, garbage reads as default, absent-and-throwing storage. The key
// string is asserted literally, so a rename is a visible break rather than a
// silently-forgotten preference.

import { describe, expect, it } from "vitest";
import { readAsideCollapsed, writeAsideCollapsed } from "./aside-prefs";
import type { StorageLike } from "../storage";

function fakeStorage(seed: Record<string, string> = {}): StorageLike & {
  entries: Record<string, string>;
} {
  const entries = { ...seed };
  return {
    entries,
    getItem: (key) => entries[key] ?? null,
    setItem: (key, value) => {
      entries[key] = value;
    },
    removeItem: (key) => {
      delete entries[key];
    },
  };
}

const throwing: StorageLike = {
  getItem: () => {
    throw new Error("nope");
  },
  setItem: () => {
    throw new Error("nope");
  },
  removeItem: () => {
    throw new Error("nope");
  },
};

const COLLAPSED_KEY = "hb.questions.aside-collapsed";

describe("aside-prefs", () => {
  it("defaults to open when nothing is stored", () => {
    expect(readAsideCollapsed(fakeStorage())).toBe(false);
  });

  it("round-trips the collapsed state", () => {
    const storage = fakeStorage();
    writeAsideCollapsed(storage, true);
    expect(storage.entries[COLLAPSED_KEY]).toBe("1");
    expect(readAsideCollapsed(storage)).toBe(true);
  });

  it("removes the key rather than storing the default, so it cannot rot", () => {
    const storage = fakeStorage({ [COLLAPSED_KEY]: "1" });
    writeAsideCollapsed(storage, false);
    expect(COLLAPSED_KEY in storage.entries).toBe(false);
    expect(readAsideCollapsed(storage)).toBe(false);
  });

  it("reads garbage as open", () => {
    for (const stored of ["", "0", "true", "yes", "{}", "11"]) {
      expect(readAsideCollapsed(fakeStorage({ [COLLAPSED_KEY]: stored }))).toBe(false);
    }
  });

  it("survives an absent storage and a throwing one", () => {
    expect(readAsideCollapsed(undefined)).toBe(false);
    expect(readAsideCollapsed(throwing)).toBe(false);
    expect(() => writeAsideCollapsed(undefined, true)).not.toThrow();
    expect(() => writeAsideCollapsed(throwing, true)).not.toThrow();
  });
});
