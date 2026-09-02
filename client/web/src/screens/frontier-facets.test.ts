// #403's facet criterion: "OR within a facet, AND across facets, unpicked
// matches everything." Stated as a test because getting it backwards is easy
// and the failure is silent — a board that shows too much reads as unfiltered.
//
// The one time-varying facet, urgency, takes `nowMs` as a parameter; nothing
// here reads a clock.

import { describe, expect, it } from "vitest";
import type { TaskItemDTO } from "../store/protocol";
import {
  applyFacets,
  contextsOf,
  facetCount,
  matchesFacets,
  NO_CONTEXT,
  NO_FACETS,
  toggleFacet,
} from "./frontier-facets";

const NOW_MS = Date.parse("2026-08-13T12:00:00");

function item(overrides: Partial<TaskItemDTO> = {}): TaskItemDTO {
  return {
    id: "id-0",
    seq: null,
    title: "untitled",
    description: null,
    stage: "ready",
    size: null,
    energy: null,
    context: null,
    priority: 0,
    projectId: null,
    projectPos: null,
    deadline: null,
    scheduledDate: null,
    source: null,
    sourceKey: null,
    sourceUrl: null,
    vaultPath: null,
    archivedAt: null,
    createdAt: 1,
    updatedAt: 1,
    version: 0,
    pending: false,
    ...overrides,
  };
}

describe("matchesFacets — the semantics", () => {
  it("matches everything when nothing is picked", () => {
    expect(matchesFacets(item(), NO_FACETS, NOW_MS)).toBe(true);
    expect(matchesFacets(item({ context: "@phone", size: "deep" }), NO_FACETS, NOW_MS)).toBe(true);
  });

  it("ORs within a facet — picking two contexts widens", () => {
    let picked = toggleFacet(NO_FACETS, "context", "@computer");
    expect(matchesFacets(item({ context: "@phone" }), picked, NOW_MS)).toBe(false);

    picked = toggleFacet(picked, "context", "@phone");
    expect(matchesFacets(item({ context: "@phone" }), picked, NOW_MS)).toBe(true);
    expect(matchesFacets(item({ context: "@computer" }), picked, NOW_MS)).toBe(true);
    expect(matchesFacets(item({ context: "@garden" }), picked, NOW_MS)).toBe(false);
  });

  it("ANDs across facets — picking a context and a size narrows", () => {
    let picked = toggleFacet(NO_FACETS, "context", "@computer");
    picked = toggleFacet(picked, "size", "quick");

    expect(matchesFacets(item({ context: "@computer", size: "quick" }), picked, NOW_MS)).toBe(true);
    // Each half alone is not enough.
    expect(matchesFacets(item({ context: "@computer", size: "deep" }), picked, NOW_MS)).toBe(false);
    expect(matchesFacets(item({ context: "@phone", size: "quick" }), picked, NOW_MS)).toBe(false);
  });

  it("treats an absent context as its own pickable value", () => {
    const picked = toggleFacet(NO_FACETS, "context", NO_CONTEXT);
    expect(matchesFacets(item({ context: null }), picked, NOW_MS)).toBe(true);
    expect(matchesFacets(item({ context: "@computer" }), picked, NOW_MS)).toBe(false);
  });

  it("excludes an unjudged size or energy once that facet is picked at all", () => {
    // `size` and `energy` have closed vocabularies and no "none" chip: picking
    // `quick` is a claim about the work's shape, and an item nobody has judged
    // makes no such claim.
    const bySize = toggleFacet(NO_FACETS, "size", "quick");
    expect(matchesFacets(item({ size: null }), bySize, NOW_MS)).toBe(false);

    const byEnergy = toggleFacet(NO_FACETS, "energy", "low");
    expect(matchesFacets(item({ energy: null }), byEnergy, NOW_MS)).toBe(false);
  });

  it("filters on urgency computed from the given clock, never a read one", () => {
    const overdue = item({ id: "a", deadline: "2020-01-01" });
    const picked = toggleFacet(NO_FACETS, "urgency", "overdue");

    expect(matchesFacets(overdue, picked, NOW_MS)).toBe(true);
    // Same item, a clock from before its deadline: no longer overdue.
    expect(matchesFacets(overdue, picked, Date.parse("2019-01-01T12:00:00"))).toBe(false);
  });
});

describe("toggleFacet", () => {
  it("adds then removes a value, without mutating what it was given", () => {
    const once = toggleFacet(NO_FACETS, "size", "deep");
    expect([...once.size]).toEqual(["deep"]);
    // The shared `NO_FACETS` constant must survive being toggled off.
    expect([...NO_FACETS.size]).toEqual([]);

    const twice = toggleFacet(once, "size", "deep");
    expect([...twice.size]).toEqual([]);
    expect([...once.size]).toEqual(["deep"]);
  });

  it("leaves the other facets alone", () => {
    const picked = toggleFacet(toggleFacet(NO_FACETS, "size", "deep"), "energy", "high");
    expect([...picked.size]).toEqual(["deep"]);
    expect([...picked.energy]).toEqual(["high"]);
    expect([...picked.context]).toEqual([]);
  });
});

describe("facetCount", () => {
  it("counts picked values across every facet — the button's badge", () => {
    expect(facetCount(NO_FACETS)).toBe(0);
    let picked = toggleFacet(NO_FACETS, "context", "@computer");
    expect(facetCount(picked)).toBe(1);
    picked = toggleFacet(picked, "context", "@phone");
    picked = toggleFacet(picked, "urgency", "overdue");
    expect(facetCount(picked)).toBe(3);
  });
});

describe("contextsOf", () => {
  it("offers the known vocabulary in its own order, extras sorted, absent last", () => {
    const contexts = contextsOf([
      item({ context: null }),
      item({ context: "@zeta" }),
      item({ context: "@phone" }),
      item({ context: "@computer" }),
      item({ context: "@alpha" }),
    ]);

    expect(contexts).toEqual(["@computer", "@phone", "@alpha", "@zeta", NO_CONTEXT]);
  });

  it("offers only what is present — the schema's context is free text", () => {
    expect(contextsOf([item({ context: "@garden" })])).toEqual(["@garden"]);
    expect(contextsOf([])).toEqual([]);
  });

  it("omits the absent-context chip when every item names one", () => {
    expect(contextsOf([item({ context: "@computer" })])).toEqual(["@computer"]);
  });
});

describe("applyFacets", () => {
  it("keeps the given order and never mutates the input", () => {
    const input = [
      item({ id: "a", context: "@computer" }),
      item({ id: "b", context: "@phone" }),
      item({ id: "c", context: "@computer" }),
    ];
    const copy = [...input];
    const picked = toggleFacet(NO_FACETS, "context", "@computer");

    expect(applyFacets(input, picked, NOW_MS).map((entry) => entry.id)).toEqual(["a", "c"]);
    expect(input).toEqual(copy);
  });

  it("returns everything when nothing is picked", () => {
    const input = [item({ id: "a" }), item({ id: "b" })];
    expect(applyFacets(input, NO_FACETS, NOW_MS)).toHaveLength(2);
  });
});
