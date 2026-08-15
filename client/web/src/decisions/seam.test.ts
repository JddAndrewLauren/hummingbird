import { afterAll, beforeEach, describe, expect, it } from "vitest";

import {
  canSubmitCapture,
  decisionsReady,
  ENERGIES,
  energyOptionsFromCore,
  FACETS,
  frontierAxesFromCore,
  initDecisions,
  orderFrontier,
  priorityRankFromCore,
  resetDecisionsForTest,
  SIZES,
  sizeOptionsFromCore,
} from "./seam";
import { priorityRank } from "../screens/priority";
import { loadDecisionsForTest } from "../test/wasm-setup";
import type { TaskItemDTO } from "../store/protocol";

// The node half of "vitest executes the seam in both environments" — this
// file runs under the default `environment: "node"`, and
// `seam.jsdom.test.ts` is the same proof under jsdom. Two files rather than
// one parameterised suite because the environment is a per-file docblock.

/** The two invisibles `str::trim` and `String.trim()` disagree about, named
 * rather than written literally — a raw one in the source is unreadable and
 * the next reader would delete it as an accident. */
const BOM = "\u{feff}";
const NEL = "\u{85}";

describe("the decision seam", () => {
  it("is already instantiated by the shared setup file", () => {
    expect(decisionsReady()).toBe(true);
  });

  it("answers the capture rule out of the core", () => {
    expect(canSubmitCapture("")).toBe(false);
    expect(canSubmitCapture("   ")).toBe(false);
    expect(canSubmitCapture("buy milk")).toBe(true);
  });

  // The core states its own blank-draft alphabet rather than inheriting
  // `str::trim`, whose set differs from `String.trim()`'s in both
  // directions (`decisions/capture.rs`). Pinned from the JS side too,
  // because this is the side that used to decide it and the side a reader
  // will assume still does.
  it("refuses a draft of nothing but invisibles, BOM included", () => {
    expect(canSubmitCapture(BOM)).toBe(false);
    expect(canSubmitCapture(NEL)).toBe(false);
    expect(canSubmitCapture(`${BOM}buy milk`)).toBe(true);
  });

  it("crosses a whole frontier's worth of items and back, ordered", () => {
    const a = syntheticItem("a", "ready");
    const b = { ...syntheticItem("b", "ready"), priority: 1 };
    expect(orderFrontier([a, b]).map((item) => item.id)).toEqual(["b", "a"]);
  });
});

// M1-3 (#501): `SIZES`/`ENERGIES`/`FACETS` stay literal TS arrays in
// `frontier-facets.ts`'s shim (the same module-evaluation-order constraint
// `field-vocabulary.ts`'s header states), pinned here against the crate
// that cannot drift from `hummingbird_domain::Size`/`Energy` or
// `decisions::vocabulary::FRONTIER_AXES` because it is built on them — the
// M1-2 review's own note that this was "the one surviving unpinned
// vocabulary copy".
describe("the seam's literal frontier-facet vocabulary, pinned against the core", () => {
  it("SIZES matches the core's size vocabulary", () => {
    expect([...SIZES]).toEqual(sizeOptionsFromCore().map((option) => option.value));
  });

  it("ENERGIES matches the core's energy vocabulary", () => {
    expect([...ENERGIES]).toEqual(energyOptionsFromCore().map((option) => option.value));
  });

  it("FACETS matches the core's frontier facet axes", () => {
    expect([...FACETS]).toEqual(frontierAxesFromCore());
  });

  // `priority.ts`'s `priorityRank` is the one vocabulary the M1-3 review
  // found still duplicated (`client/core/src/decisions/frontier.rs`'s own
  // `priority_rank`, unpinned) — pinned here the same way the three literal
  // arrays above are.
  it("priorityRank matches the core's priority rank, for every real value and an unrecognised one", () => {
    for (const raw of [0, 1, 2, 3, 4, 5, -1]) {
      expect(priorityRank(raw)).toEqual(priorityRankFromCore(raw));
    }
  });
});

describe("the loading gate", () => {
  beforeEach(() => {
    resetDecisionsForTest();
  });

  afterAll(async () => {
    // Leave the module loaded for anything that runs after this file's
    // suites — the setup file's `beforeAll` has already fired.
    await initDecisions(loadDecisionsForTest);
  });

  it("throws rather than falling back to a TS copy when used too early", () => {
    expect(decisionsReady()).toBe(false);
    expect(() => canSubmitCapture("buy milk")).toThrow(/before initDecisions/);
  });

  it("instantiates once for concurrent callers", async () => {
    let loads = 0;
    const counted = async () => {
      loads += 1;
      return loadDecisionsForTest();
    };
    await Promise.all([initDecisions(counted), initDecisions(counted), initDecisions(counted)]);
    expect(loads).toBe(1);
    expect(decisionsReady()).toBe(true);
  });
});

/** The main thread's own `TaskItemDTO` shape (camelCase, `store/protocol.ts`)
 * — what M1-3's per-render calls would actually cross. */
function syntheticItem(id: string, stage: TaskItemDTO["stage"]): TaskItemDTO {
  return {
    id,
    seq: 42,
    title: "buy milk",
    description: null,
    stage,
    size: "quick",
    energy: "low",
    context: "@errands",
    priority: 2,
    projectId: null,
    projectPos: null,
    deadline: "2026-08-20",
    scheduledDate: null,
    source: "web/v1",
    sourceKey: null,
    sourceUrl: null,
    archivedAt: null,
    createdAt: 1_755_000_000_000,
    updatedAt: 1_755_000_000_000,
    version: 1,
    pending: false,
  };
}
