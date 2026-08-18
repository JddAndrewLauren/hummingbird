import { afterAll, beforeEach, describe, expect, it } from "vitest";

import {
  canSubmitCapture,
  declineForResponseFromCore,
  declineForTransportFromCore,
  decisionsReady,
  ENERGIES,
  energyOptionsFromCore,
  FACETS,
  frontierAxesFromCore,
  initDecisions,
  noTerminalLineDeclineFromCore,
  noTokenDeclineFromCore,
  orderFrontier,
  outsideSchemaDeclineFromCore,
  priorityRankFromCore,
  resetDecisionsForTest,
  paneBandOrderFromCore,
  paneQuestionOrderFromCore,
  SIZES,
  sizeOptionsFromCore,
  wasteConstantsFromCore,
} from "./seam";
import { priorityRank } from "../screens/priority";
import { declineForResponse, declineForTransport, NO_TERMINAL_LINE, NO_TOKEN } from "../skills/decline";
import { OUTSIDE_SCHEMA } from "../skills/grill-turn-state";
import { BAND_ORDER, QUESTION_ORDER } from "../screens/questions/contract";
import {
  BINDING_KEY,
  SNAPSHOT_KEY,
  SOURCE,
  STALE_AFTER_MS,
  STREAM_ORDER,
} from "../screens/waste-pane/waste";
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

// M4 (#538): the skills lane's three module-evaluation-time constants. Same
// carve-out, same reason, same pin — `NO_TOKEN` and `NO_TERMINAL_LINE` are
// read at module-evaluation time by `route-run.ts` and
// `useMicrotaskWiring.ts`, and `OUTSIDE_SCHEMA` by `useGrillWiring.ts`, all
// statically reachable from `main.tsx`, so a seam call there would throw the
// "used before ready" guard on every page load. Everything else in that lane
// calls across; only these three stay literal, and only because they cannot.
describe("the skills lane's literal decline prose, pinned against the core", () => {
  it("NO_TOKEN matches the core's words", () => {
    expect(NO_TOKEN).toBe(noTokenDeclineFromCore());
  });

  it("NO_TERMINAL_LINE matches the core's words", () => {
    expect(NO_TERMINAL_LINE).toBe(noTerminalLineDeclineFromCore());
  });

  it("OUTSIDE_SCHEMA matches the core's words", () => {
    expect(OUTSIDE_SCHEMA).toBe(outsideSchemaDeclineFromCore());
  });

  /** The two that are *not* constants — proof they were not quietly copied
   * alongside the three that had to be. */
  it("the two decline functions answer out of the core, not a TS copy", () => {
    expect(declineForResponse(401)).toBe(declineForResponseFromCore(401));
    expect(declineForResponse(503)).toBe(declineForResponseFromCore(503));
    expect(declineForTransport("boom")).toBe(declineForTransportFromCore("boom"));
  });
});

// M4 (#533): the panes' vocabularies. `BAND_ORDER` and `QUESTION_ORDER`
// stay literal TS in `contract.ts` for a HARDER version of the same
// module-evaluation-order constraint as above — `registry.ts` builds its
// `QUESTIONS` map at module evaluation and reads `QUESTION_ORDER` there, so
// a seam call would throw the "used before ready" guard on every page load,
// not merely in a test. The waste pane's four constants are the same story
// via `question.ts`'s `sources: [SOURCE]`. All of them are pinned here
// instead.
describe("the seam's literal pane vocabulary, pinned against the core", () => {
  it("BAND_ORDER matches the core's salience vocabulary, in order", () => {
    expect([...BAND_ORDER]).toEqual(paneBandOrderFromCore());
  });

  it("QUESTION_ORDER matches the core's declared question order", () => {
    // Declaration order, not alphabetical — a question's place must not
    // move when another is renamed, and the two clients must agree which
    // order that is.
    expect([...QUESTION_ORDER]).toEqual(paneQuestionOrderFromCore());
  });

  it("the waste pane's constants match the core's", () => {
    const constants = wasteConstantsFromCore();
    expect(SOURCE).toBe(constants.source);
    expect(SNAPSHOT_KEY).toBe(constants.snapshotKey);
    expect(BINDING_KEY).toBe(constants.bindingKey);
    // ADR-0015 puts the threshold beside the band function, and ADR-0025
    // moved the pair into the core — this is the copy that must not drift
    // from it.
    expect(STALE_AFTER_MS).toBe(constants.staleAfterMs);
    expect([...STREAM_ORDER]).toEqual(constants.streamOrder);
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
