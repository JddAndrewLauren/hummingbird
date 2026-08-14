// #403's preference criteria: "a pure sibling module over an injectable
// storage, with the five-test template `shell/rail-collapse.test.ts` establishes:
// default, round-trip, key-removal-not-default, garbage reads as default,
// absent-and-throwing storage" — run once for the axis and once for the
// collapsed set.
//
// The key strings are asserted **literally**, per #403, so a rename is a
// visible break rather than a silently-forgotten preference.

import { describe, expect, it } from "vitest";
import {
  readCollapsedColumns,
  readFrontierAxis,
  writeCollapsedColumns,
  writeFrontierAxis,
} from "./frontier-prefs";
import type { StorageLike } from "./storage";

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

const AXIS_KEY = "hb.now.frontier-axis";
const COLLAPSED_KEY = "hb.now.frontier-collapsed";

describe("frontier-prefs — the grouping axis", () => {
  it("defaults to context when nothing is stored", () => {
    expect(readFrontierAxis(fakeStorage())).toBe("context");
  });

  it("round-trips a non-default axis", () => {
    const storage = fakeStorage();
    writeFrontierAxis(storage, "energy");
    expect(storage.entries[AXIS_KEY]).toBe("energy");
    expect(readFrontierAxis(storage)).toBe("energy");
  });

  it("removes the key rather than storing the default, so it cannot rot", () => {
    const storage = fakeStorage({ [AXIS_KEY]: "size" });
    writeFrontierAxis(storage, "context");
    expect(AXIS_KEY in storage.entries).toBe(false);
    expect(readFrontierAxis(storage)).toBe("context");
  });

  it("degrades an unknown stored axis to the default", () => {
    // A newer build's vocabulary, or a hand-edited key. #403: "An unknown
    // stored axis degrades to the default."
    expect(readFrontierAxis(fakeStorage({ [AXIS_KEY]: "urgency" }))).toBe("context");
    expect(readFrontierAxis(fakeStorage({ [AXIS_KEY]: "" }))).toBe("context");
    expect(readFrontierAxis(fakeStorage({ [AXIS_KEY]: "CONTEXT" }))).toBe("context");
  });

  it("survives an absent storage and a throwing one", () => {
    expect(readFrontierAxis(undefined)).toBe("context");
    expect(readFrontierAxis(throwing)).toBe("context");
    expect(() => writeFrontierAxis(undefined, "size")).not.toThrow();
    expect(() => writeFrontierAxis(throwing, "size")).not.toThrow();
  });
});

describe("frontier-prefs — the collapsed columns", () => {
  it("defaults to nothing collapsed when nothing is stored", () => {
    expect([...readCollapsedColumns(fakeStorage())]).toEqual([]);
  });

  it("round-trips a set of column labels", () => {
    const storage = fakeStorage();
    writeCollapsedColumns(storage, new Set(["@computer", "no context"]));
    expect(storage.entries[COLLAPSED_KEY]).toBe('["@computer","no context"]');
    expect([...readCollapsedColumns(storage)].sort()).toEqual(["@computer", "no context"]);
  });

  it("removes the key rather than storing an empty list", () => {
    const storage = fakeStorage({ [COLLAPSED_KEY]: '["@computer"]' });
    writeCollapsedColumns(storage, new Set());
    expect(COLLAPSED_KEY in storage.entries).toBe(false);
    expect([...readCollapsedColumns(storage)]).toEqual([]);
  });

  it("reads garbage as nothing collapsed", () => {
    // Unparseable, and parseable-but-wrong-shape: a view preference is never
    // worth an error, and the default rule is always a correct answer.
    for (const stored of ["not json", "{}", '"a string"', "42", '["ok", 7]', "null"]) {
      expect([...readCollapsedColumns(fakeStorage({ [COLLAPSED_KEY]: stored }))]).toEqual([]);
    }
  });

  it("survives an absent storage and a throwing one", () => {
    expect([...readCollapsedColumns(undefined)]).toEqual([]);
    expect([...readCollapsedColumns(throwing)]).toEqual([]);
    expect(() => writeCollapsedColumns(undefined, new Set(["a"]))).not.toThrow();
    expect(() => writeCollapsedColumns(throwing, new Set(["a"]))).not.toThrow();
  });

  it("keeps the two preferences in separate keys", () => {
    // Writing one must never disturb the other — they are cleared on different
    // beats (the axis persists; changing it wipes the collapsed set).
    const storage = fakeStorage();
    writeFrontierAxis(storage, "size");
    writeCollapsedColumns(storage, new Set(["quick"]));
    expect(Object.keys(storage.entries).sort()).toEqual([AXIS_KEY, COLLAPSED_KEY]);
  });
});
