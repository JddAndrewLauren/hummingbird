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
const board = [...state.frontier, ...state.triageInbox];

const tally = (values: Array<string | null>) =>
  values.reduce<Record<string, number>>((acc, value) => {
    const key = value ?? "(none)";
    acc[key] = (acc[key] ?? 0) + 1;
    return acc;
  }, {});

describe("buildDemoTaskState — production's shape, none of its content", () => {
  it("puts 29 cards on the board, 12 startable and 17 unsorted", () => {
    expect(state.frontier).toHaveLength(12);
    expect(state.triageInbox).toHaveLength(17);
    expect(board).toHaveLength(29);
  });

  it("mirrors the context spread, so the no-context column really is the biggest", () => {
    expect(tally(board.map((i) => i.context))).toEqual({
      "(none)": 12,
      "@computer": 8,
      "@errands": 4,
      "@phone": 3,
      "@home": 2,
    });
  });

  it("mirrors the size and energy spread, both dominated by unset", () => {
    expect(tally(board.map((i) => i.size))).toEqual({
      "(none)": 13,
      deep: 8,
      quick: 4,
      short: 4,
    });
    expect(tally(board.map((i) => i.energy))).toEqual({
      "(none)": 17,
      high: 5,
      medium: 4,
      low: 3,
    });
  });

  it("mirrors the capture sources — most typed, seven swept from mail", () => {
    expect(tally(board.map((i) => i.source))).toEqual({
      "(none)": 21,
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

describe("buildDemoTaskState — the two deliberate departures from production", () => {
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
