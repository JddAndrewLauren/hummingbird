import { describe, expect, it } from "vitest";
import type { TaskItemDTO } from "../store/protocol";
import {
  applyItemAction,
  availableActions,
  canMarkDone,
  resolveFallbackPending,
} from "./item-actions";

const baseItem: TaskItemDTO = {
  id: "item-1",
  seq: 1,
  title: "buy milk",
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
  archivedAt: null,
  createdAt: 1_000,
  updatedAt: 1_000,
  version: 1,
  pending: false,
};

describe("availableActions", () => {
  it("offers nothing for a pre-action stage", () => {
    expect(availableActions("triage")).toEqual([]);
    expect(availableActions("grilling")).toEqual([]);
  });

  it("offers start, complete, block and cancel for a Ready item", () => {
    // `complete` from Ready is the row-checkmark amendment: finishing is one
    // click from any live stage, never gated on having said "start" first.
    expect(availableActions("ready")).toEqual(["start", "complete", "block", "cancel"]);
  });

  it("offers complete, block and cancel for an In Progress item, never start", () => {
    const actions = availableActions("in_progress");
    expect(actions).toEqual(["complete", "block", "cancel"]);
    expect(actions).not.toContain("start");
  });

  it("offers start, complete and cancel for a Blocked item, but never block again", () => {
    const actions = availableActions("blocked");
    expect(actions).toEqual(["start", "complete", "cancel"]);
    expect(actions).not.toContain("block");
  });

  it("offers nothing for a finished item", () => {
    expect(availableActions("done")).toEqual([]);
  });
});

// The one deciding function for which rows carry the mark-done checkmark —
// wider than `availableActions` on purpose (Triage/Grilling rows get it even
// though the detail panel's vocabulary still offers them nothing).
describe("canMarkDone", () => {
  it("allows every live, unarchived stage — Triage and Grilling included", () => {
    for (const stage of ["triage", "grilling", "ready", "in_progress", "blocked"] as const) {
      expect(canMarkDone({ stage, archivedAt: null })).toBe(true);
    }
  });

  it("refuses a finished item — there is nothing left to mark", () => {
    expect(canMarkDone({ stage: "done", archivedAt: null })).toBe(false);
  });

  it("refuses an archived item — cancelled is settled, not completable", () => {
    expect(canMarkDone({ stage: "ready", archivedAt: 2_000 })).toBe(false);
  });
});

describe("applyItemAction", () => {
  it("start moves the item to in_progress and marks it pending", () => {
    expect(applyItemAction(baseItem, "start")).toEqual({
      ...baseItem,
      stage: "in_progress",
      pending: true,
    });
  });

  it("complete moves the item to done and marks it pending", () => {
    expect(applyItemAction(baseItem, "complete")).toEqual({
      ...baseItem,
      stage: "done",
      pending: true,
    });
  });

  it("block moves the item to blocked and marks it pending, never a relation", () => {
    expect(applyItemAction(baseItem, "block")).toEqual({
      ...baseItem,
      stage: "blocked",
      pending: true,
    });
  });

  it("cancel archives the item without touching its stage", () => {
    const result = applyItemAction(baseItem, "cancel");
    expect(result.stage).toBe("ready");
    expect(result.archivedAt).not.toBeNull();
    expect(result.pending).toBe(true);
  });
});

// The round-2 reviewer finding on PR #207: `applyItemAction`'s frozen
// `pending: true` snapshot never updated again, so the fallback panel's
// Start/Cancel row on a just-blocked item rendered but stayed disabled for
// the whole session. `resolveFallbackPending` is the fix's deciding logic —
// a pure function (this repo has no component-test infra, so this is the
// executable coverage; `NowScreen.tsx` only threads state through it).
describe("resolveFallbackPending", () => {
  it("stays disabled on the frozen optimistic pending while no live answer exists yet", () => {
    expect(resolveFallbackPending(true, undefined, false)).toEqual({
      pending: true,
      awaitingConfirm: false,
    });
  });

  it("enables the row once the live read reports the mutation drained", () => {
    // This is the exact defect: before the fix nothing could ever turn the
    // frozen `pending: true` off, so the blocked item's Start/Cancel row was
    // functionally unreachable.
    expect(resolveFallbackPending(true, false, false)).toEqual({
      pending: false,
      awaitingConfirm: false,
    });
  });

  it("keeps a fresh act disabled through a stale pre-act live false", () => {
    // Clicking Start on an enabled blocked row leaves `task.pending[id]`
    // holding the PREVIOUS act's drained `false` until the new act's own
    // `isPending` read lands — awaitingConfirm bridges that window so the
    // stale value cannot re-enable the row mid-mutation.
    expect(resolveFallbackPending(true, false, true)).toEqual({
      pending: true,
      awaitingConfirm: true,
    });
  });

  it("clears awaitingConfirm the moment the live read confirms the act is queued", () => {
    expect(resolveFallbackPending(true, true, true)).toEqual({
      pending: true,
      awaitingConfirm: false,
    });
  });

  it("trusts a live pending true over a stale optimistic false", () => {
    expect(resolveFallbackPending(false, true, false)).toEqual({
      pending: true,
      awaitingConfirm: false,
    });
  });
});
