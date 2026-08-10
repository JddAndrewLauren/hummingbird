import { describe, expect, it } from "vitest";
import type { TaskItemDTO } from "../store/protocol";
import { applyItemAction, availableActions } from "./item-actions";

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

  it("offers start, block and cancel for a Ready item", () => {
    expect(availableActions("ready")).toEqual(["start", "block", "cancel"]);
  });

  it("offers complete, block and cancel for an In Progress item, never start", () => {
    const actions = availableActions("in_progress");
    expect(actions).toEqual(["complete", "block", "cancel"]);
    expect(actions).not.toContain("start");
  });

  it("offers start and cancel for a Blocked item, but never block again", () => {
    const actions = availableActions("blocked");
    expect(actions).toEqual(["start", "cancel"]);
    expect(actions).not.toContain("block");
  });

  it("offers nothing for a finished item", () => {
    expect(availableActions("done")).toEqual([]);
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
