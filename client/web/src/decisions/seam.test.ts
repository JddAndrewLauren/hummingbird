import { afterAll, beforeEach, describe, expect, it } from "vitest";

import {
  canSubmitCapture,
  decisionsReady,
  initDecisions,
  probeItemPayload,
  resetDecisionsForTest,
} from "./seam";
import { loadDecisionsForTest } from "../test/wasm-setup";

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

  it("crosses a structured payload and back", () => {
    const payload = JSON.stringify([syntheticItem("a", "ready"), syntheticItem("b", "done")]);
    expect(JSON.parse(probeItemPayload(payload))).toEqual({ count: 2, open: 1 });
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
function syntheticItem(id: string, stage: string) {
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
