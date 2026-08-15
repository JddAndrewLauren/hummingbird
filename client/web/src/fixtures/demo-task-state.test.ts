import { describe, expect, it } from "vitest";
import { buildDemoTaskState } from "./demo-task-state";
import { computeUrgency, isValidDeadline } from "../screens/urgency";

// The board fixture's header states production's measured shape and claims to
// mirror it. These assertions are what stop that claim rotting into a
// decoration: an item edited without minding the distribution fails here,
// which is the moment to either fix the item or re-measure and rewrite the
// header. The numbers came from `GET /api/changes?since=0` on 2026-08-13.

// Built once here rather than per test: `buildDemoTaskState` reads the clock,
// and every assertion below is about shape, which no two calls disagree on.
const state = buildDemoTaskState();
// #357: the one seeded Grilling item is departure 3, not part of production's
// measured shape (the header's table stays the 29-card measurement) — but it
// IS on Now's board (`triageProcessQueue` combines it with `triageInbox`), so
// it belongs in every assertion below that is about what actually renders.
const board = [...state.frontier, ...state.triageInbox, ...state.grillingItems];

const tally = (values: Array<string | null>) =>
  values.reduce<Record<string, number>>((acc, value) => {
    const key = value ?? "(none)";
    acc[key] = (acc[key] ?? 0) + 1;
    return acc;
  }, {});

describe("buildDemoTaskState — production's shape, none of its content", () => {
  it("puts 30 cards on the board: 12 startable, 17 captured, 1 grilling", () => {
    expect(state.frontier).toHaveLength(12);
    expect(state.triageInbox).toHaveLength(17);
    expect(state.grillingItems).toHaveLength(1);
    expect(board).toHaveLength(30);
  });

  it("mirrors the context spread, so the no-context column really is the biggest", () => {
    // The seeded Grilling item carries no context, same as most of production.
    expect(tally(board.map((i) => i.context))).toEqual({
      "(none)": 13,
      "@computer": 8,
      "@errands": 4,
      "@phone": 3,
      "@home": 2,
    });
  });

  it("mirrors the size and energy spread, both dominated by unset", () => {
    // The seeded Grilling item is `size: "deep"`, `energy` unset.
    expect(tally(board.map((i) => i.size))).toEqual({
      "(none)": 13,
      deep: 9,
      quick: 4,
      normal: 4,
    });
    expect(tally(board.map((i) => i.energy))).toEqual({
      "(none)": 18,
      high: 5,
      medium: 4,
      low: 3,
    });
  });

  it("mirrors the capture sources — most typed, seven swept from mail", () => {
    expect(tally(board.map((i) => i.source))).toEqual({
      "(none)": 22,
      "gmail/v1": 7,
      "google-tasks/v1": 1,
    });
  });

  it("has no projects and no blocked edges, which is why the Project axis is one column", () => {
    expect(state.projects).toEqual([]);
    expect(state.blocked).toEqual([]);
    expect(board.every((i) => i.projectId === null)).toBe(true);
  });

  it("leaves priority at production's own flat zero", () => {
    expect(board.every((i) => i.priority === 0)).toBe(true);
  });

  it("overflows two context columns past the six-card cap, so `n more` is photographed", () => {
    const overCap = Object.entries(tally(board.map((i) => i.context))).filter(
      ([, count]) => count > 6,
    );
    expect(overCap.map(([context]) => context).sort()).toEqual(["(none)", "@computer"]);
  });
});

describe("buildDemoTaskState — the three deliberate departures from production", () => {
  it("carries one deadline per urgency band, which production's single deadline cannot", () => {
    const nowMs = Date.now();
    const bands = board
      .map((i) => computeUrgency(i.deadline, nowMs))
      .filter((band) => band !== "calm")
      .sort();
    expect(bands).toEqual(["now", "overdue", "soon"]);
  });

  it("spells every deadline the one way ADR-0009/0013 allows", () => {
    for (const item of board) {
      if (item.deadline !== null) {
        expect(isValidDeadline(item.deadline)).toBe(true);
      }
    }
  });

  it("seeds a failed triage naming a capture that is on the board, so #418's alert renders", () => {
    const failure = state.lastTriage;
    expect(failure?.kind).toBe("failed");
    // Naming an item that is NOT in the inbox would silently degrade the
    // alert to its un-named fallback — the fixture would still "work" and
    // would stop demonstrating the sentence it exists to demonstrate.
    expect(state.triageInbox.some((i) => i.id === failure?.itemId)).toBe(true);
  });

  it("seeds one Grilling-stage item, so #357's queue and its badge both render", () => {
    expect(state.grillingItems).toHaveLength(1);
    expect(state.grillingItems[0].stage).toBe("grilling");
  });
});

describe("buildDemoTaskState — ages stay honest", () => {
  it("dates every item relative to load, so nothing ever reads `412d ago`", () => {
    const nowMs = Date.now();
    for (const item of board) {
      expect(item.createdAt).toBeLessThanOrEqual(nowMs);
      // Nothing older than the ~30d the oldest seed asks for, with slack.
      expect(nowMs - item.createdAt).toBeLessThan(31 * 24 * 60 * 60 * 1000);
    }
  });

  it("gives every item a unique id, so React keys and selection cannot collide", () => {
    expect(new Set(board.map((i) => i.id)).size).toBe(board.length);
  });
});

describe("buildDemoTaskState — #452 grows the seed past the frontier and the inbox", () => {
  it("seeds Rules with both fields the screen gates on", () => {
    expect(state.rules).not.toBeNull();
    expect(state.rules?.length).toBeGreaterThan(0);
    expect(state.kindRegistry).not.toBeNull();
  });

  it("keeps projects and blocked at production's own measured zero", () => {
    expect(state.projects).toEqual([]);
    expect(state.blocked).toEqual([]);
  });

  it("seeds Done with real, populated items — not the honest empty state", () => {
    expect(state.done).not.toBeNull();
    expect(state.done?.length).toBeGreaterThan(0);
    expect(state.done?.every((i) => i.stage === "done")).toBe(true);
  });

  it("seeds the Ledger as a superset of the frontier, the inbox, Grilling and Done, plus archived-only rows", () => {
    expect(state.ledger).not.toBeNull();
    const ledgerIds = new Set(state.ledger?.map((row) => row.id));
    for (const item of [
      ...state.frontier,
      ...state.triageInbox,
      ...state.grillingItems,
      ...(state.done ?? []),
    ]) {
      expect(ledgerIds.has(item.id)).toBe(true);
    }
    // At least one archived-only row: present in the Ledger, absent from every
    // live list.
    const archivedOnly = state.ledger?.filter(
      (row) =>
        row.archivedAt !== null &&
        !state.frontier.some((i) => i.id === row.id) &&
        !state.triageInbox.some((i) => i.id === row.id) &&
        !state.grillingItems.some((i) => i.id === row.id) &&
        !(state.done ?? []).some((i) => i.id === row.id),
    );
    expect(archivedOnly?.length).toBeGreaterThan(0);
  });

  it("counts the seeded Grilling item into lastSyncOutcome.activeItemCount, mirroring SyncMirror::active_item_count's every-stage-but-Done filter", () => {
    expect(state.lastSyncOutcome?.activeItemCount).toBe(
      state.frontier.length + state.triageInbox.length + state.grillingItems.length,
    );
  });

  it("gives the Ledger at least one deadLettered row and one hasLiveAlert row, so both badges render", () => {
    expect(state.ledger?.some((row) => row.deadLettered)).toBe(true);
    expect(state.ledger?.some((row) => row.hasLiveAlert)).toBe(true);
  });

  it("seeds every known binding key this build actually uses, all set", () => {
    const keys = state.bindings?.map((b) => b.key).sort();
    expect(keys).toEqual(["city-waste-page", "race-series", "trips-calendar"]);
    expect(state.bindings?.every((b) => b.known)).toBe(true);
  });

  it("seeds a pane read for every standing-question source the ranked region reads", () => {
    expect(Object.keys(state.paneReads).sort()).toEqual(
      [
        "city-waste/v2",
        "race-schedule/v1",
        "github-hummingbird/v1",
        "kimi-balance/v1",
        "uptime/v1",
      ].sort(),
    );
  });

  it("seeds a recent completed sync, so the reachability pane answers rather than saying never synced", () => {
    expect(state.lastSyncOutcome?.kind).toBe("completed");
    expect(state.lastSyncAtMs).not.toBeNull();
    expect(state.lastSuccessfulSyncAtMs).not.toBeNull();
  });
});
