// #403's preference criteria: "a pure sibling module over an injectable
// storage, with the five-test template `shell/rail-collapse.test.ts` establishes:
// default, round-trip, key-removal-not-default, garbage reads as default,
// absent-and-throwing storage" — run once for the axis and once for the
// collapsed set.
//
// The key strings are asserted **literally**, per #403, so a rename is a
// visible break rather than a silently-forgotten preference.
//
// Every call now names its screen: the board is on Now and on a project's
// dossier, and the last two cases here are what stops the second surface
// quietly sharing the first's keys or grouping by an axis it does not offer.

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
    expect(readFrontierAxis(fakeStorage(), "now")).toBe("context");
  });

  it("round-trips a non-default axis", () => {
    const storage = fakeStorage();
    writeFrontierAxis(storage, "now", "energy");
    expect(storage.entries[AXIS_KEY]).toBe("energy");
    expect(readFrontierAxis(storage, "now")).toBe("energy");
  });

  it("removes the key rather than storing the default, so it cannot rot", () => {
    const storage = fakeStorage({ [AXIS_KEY]: "size" });
    writeFrontierAxis(storage, "now", "context");
    expect(AXIS_KEY in storage.entries).toBe(false);
    expect(readFrontierAxis(storage, "now")).toBe("context");
  });

  it("degrades an unknown stored axis to the default", () => {
    // A newer build's vocabulary, or a hand-edited key. #403: "An unknown
    // stored axis degrades to the default."
    expect(readFrontierAxis(fakeStorage({ [AXIS_KEY]: "urgency" }), "now")).toBe("context");
    expect(readFrontierAxis(fakeStorage({ [AXIS_KEY]: "" }), "now")).toBe("context");
    expect(readFrontierAxis(fakeStorage({ [AXIS_KEY]: "CONTEXT" }), "now")).toBe("context");
  });

  it("survives an absent storage and a throwing one", () => {
    expect(readFrontierAxis(undefined, "now")).toBe("context");
    expect(readFrontierAxis(throwing, "now")).toBe("context");
    expect(() => writeFrontierAxis(undefined, "now", "size")).not.toThrow();
    expect(() => writeFrontierAxis(throwing, "now", "size")).not.toThrow();
  });
});

describe("frontier-prefs — the collapsed columns", () => {
  it("defaults to nothing collapsed when nothing is stored", () => {
    expect([...readCollapsedColumns(fakeStorage(), "now")]).toEqual([]);
  });

  it("round-trips a set of column labels", () => {
    const storage = fakeStorage();
    writeCollapsedColumns(storage, "now", new Set(["@computer", "no context"]));
    expect(storage.entries[COLLAPSED_KEY]).toBe('["@computer","no context"]');
    expect([...readCollapsedColumns(storage, "now")].sort()).toEqual(["@computer", "no context"]);
  });

  it("removes the key rather than storing an empty list", () => {
    const storage = fakeStorage({ [COLLAPSED_KEY]: '["@computer"]' });
    writeCollapsedColumns(storage, "now", new Set());
    expect(COLLAPSED_KEY in storage.entries).toBe(false);
    expect([...readCollapsedColumns(storage, "now")]).toEqual([]);
  });

  it("reads garbage as nothing collapsed", () => {
    // Unparseable, and parseable-but-wrong-shape: a view preference is never
    // worth an error, and the default rule is always a correct answer.
    for (const stored of ["not json", "{}", '"a string"', "42", '["ok", 7]', "null"]) {
      expect([...readCollapsedColumns(fakeStorage({ [COLLAPSED_KEY]: stored }), "now")]).toEqual([]);
    }
  });

  it("survives an absent storage and a throwing one", () => {
    expect([...readCollapsedColumns(undefined, "now")]).toEqual([]);
    expect([...readCollapsedColumns(throwing, "now")]).toEqual([]);
    expect(() => writeCollapsedColumns(undefined, "now", new Set(["a"]))).not.toThrow();
    expect(() => writeCollapsedColumns(throwing, "now", new Set(["a"]))).not.toThrow();
  });

  it("keeps the two preferences in separate keys", () => {
    // Writing one must never disturb the other — they are cleared on different
    // beats (the axis persists; changing it wipes the collapsed set).
    const storage = fakeStorage();
    writeFrontierAxis(storage, "now", "size");
    writeCollapsedColumns(storage, "now", new Set(["quick"]));
    expect(Object.keys(storage.entries).sort()).toEqual([AXIS_KEY, COLLAPSED_KEY]);
  });
});

describe("frontier-prefs — the two boards", () => {
  it("keeps each screen's preferences in its own key namespace", () => {
    // The failure this rules out: switching the project board's axis
    // re-grouping Now behind the reader's back.
    const storage = fakeStorage();
    writeFrontierAxis(storage, "projects", "size");
    writeCollapsedColumns(storage, "projects", new Set(["quick"]));
    expect(Object.keys(storage.entries).sort()).toEqual([
      "hb.projects.frontier-axis",
      "hb.projects.frontier-collapsed",
    ]);
    expect(readFrontierAxis(storage, "now")).toBe("context");
    expect([...readCollapsedColumns(storage, "now")]).toEqual([]);
    expect(readFrontierAxis(storage, "projects")).toBe("size");
  });

  it("degrades a stored axis the board does not offer", () => {
    // The project board drops the `project` axis (one column, always), so a
    // `project` stored by a build that offered it — or by Now, had the keys
    // been shared — must not group by a button that is not on screen.
    const storage = fakeStorage({ "hb.projects.frontier-axis": "project" });
    expect(readFrontierAxis(storage, "projects")).toBe("project");
    expect(readFrontierAxis(storage, "projects", ["context", "size", "energy"])).toBe("context");
  });
});
