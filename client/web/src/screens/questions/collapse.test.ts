import { describe, expect, it } from "vitest";
import type { Band, PaneAnswer } from "./contract";
import {
  defaultCollapsed,
  readCollapseMap,
  resolveCollapsed,
  writeCollapseOverride,
  type CollapseMap,
  type StorageLike,
} from "./collapse";

function fakeStorage(seed: Record<string, string> = {}): StorageLike & { read(): Record<string, string> } {
  const store = new Map(Object.entries(seed));
  return {
    getItem: (key) => store.get(key) ?? null,
    setItem: (key, value) => void store.set(key, value),
    removeItem: (key) => void store.delete(key),
    read: () => Object.fromEntries(store),
  };
}

function answer(band: Band, answerState: PaneAnswer["answerState"] = "answered"): PaneAnswer {
  return { answerState, band, withinBand: 0, collapsedHeadline: "a line" };
}

describe("defaultCollapsed", () => {
  it("collapses a dormant pane and opens every livelier band", () => {
    expect(defaultCollapsed(answer("dormant"))).toBe(true);
    for (const band of ["live", "imminent", "near", "distant"] as const) {
      expect(defaultCollapsed(answer(band))).toBe(false);
    }
  });

  it("collapses a gap and an unbound question whatever their band", () => {
    expect(defaultCollapsed(answer("live", "bound-but-unacquired"))).toBe(true);
    expect(defaultCollapsed(answer("live", "unbound"))).toBe(true);
  });
});

describe("resolveCollapsed", () => {
  it("applies an override in either direction while the band still matches", () => {
    const stored: CollapseMap = {
      "waste:collection": { band: "dormant", collapsed: false },
      "race:f1": { band: "imminent", collapsed: true },
    };
    // A dormant pane held open, and an imminent one held shut: the override
    // works both ways, which is why it stores a direction rather than a flag.
    expect(resolveCollapsed(stored, "waste:collection", answer("dormant"))).toBe(false);
    expect(resolveCollapsed(stored, "race:f1", answer("imminent"))).toBe(true);
  });

  it("stops applying once the pane's band moves, and falls back to the default", () => {
    const stored: CollapseMap = { "waste:collection": { band: "dormant", collapsed: true } };
    expect(resolveCollapsed(stored, "waste:collection", answer("imminent"))).toBe(false);
  });

  it("resurrects the override when the band comes back", () => {
    // The band-mismatch above is a read-time non-match, NOT a deletion —
    // which is the whole reason collapse state is band-scoped rather than
    // cleared on every band change. Collapse it while dormant, watch the
    // collection get close, watch it recede: still collapsed.
    const storage = fakeStorage();
    const stored = writeCollapseOverride(
      storage,
      {},
      "waste:collection",
      { band: "dormant", collapsed: true },
      ["waste:collection"],
    );
    expect(resolveCollapsed(stored, "waste:collection", answer("imminent"))).toBe(false);
    expect(resolveCollapsed(stored, "waste:collection", answer("dormant"))).toBe(true);
  });

  it("uses the default for a pane nobody has ever overridden", () => {
    expect(resolveCollapsed({}, "waste:collection", answer("dormant"))).toBe(true);
    expect(resolveCollapsed({}, "waste:collection", answer("imminent"))).toBe(false);
  });
});

describe("writeCollapseOverride", () => {
  it("round-trips through storage", () => {
    const storage = fakeStorage();
    writeCollapseOverride(storage, {}, "waste:collection", { band: "dormant", collapsed: true }, [
      "waste:collection",
    ]);
    expect(readCollapseMap(storage)).toEqual({
      "waste:collection": { band: "dormant", collapsed: true },
    });
  });

  it("prunes entries for panes that are no longer ranked, and keeps band-mismatched ones", () => {
    const storage = fakeStorage();
    const current: CollapseMap = {
      "gone:subject": { band: "near", collapsed: true },
      "waste:collection": { band: "dormant", collapsed: true },
    };
    const next = writeCollapseOverride(
      storage,
      current,
      "race:f1",
      { band: "imminent", collapsed: false },
      ["waste:collection", "race:f1"],
    );
    expect(Object.keys(next).sort()).toEqual(["race:f1", "waste:collection"]);
    // The kept one is stored against a band nothing is in right now — that
    // is the resurrection case, not stale data.
    expect(next["waste:collection"]).toEqual({ band: "dormant", collapsed: true });
    expect(readCollapseMap(storage)).toEqual(next);
  });
});

describe("readCollapseMap", () => {
  it("reads anything unusable as an empty map rather than failing", () => {
    for (const raw of ["", "not json", "[]", "7", '{"waste:collection":{"band":"nope"}}']) {
      expect(readCollapseMap(fakeStorage({ "hb.questions.collapse": raw }))).toEqual({});
    }
    expect(readCollapseMap(fakeStorage())).toEqual({});
  });

  it("keeps the readable entries beside an unreadable one", () => {
    const storage = fakeStorage({
      "hb.questions.collapse": JSON.stringify({
        "waste:collection": { band: "dormant", collapsed: true },
        "junk:subject": { band: "dormant" },
      }),
    });
    expect(readCollapseMap(storage)).toEqual({
      "waste:collection": { band: "dormant", collapsed: true },
    });
  });
});
