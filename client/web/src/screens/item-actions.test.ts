import { describe, expect, it } from "vitest";
import { availableActions } from "./item-actions";

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
