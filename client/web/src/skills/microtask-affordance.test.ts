import { describe, expect, it } from "vitest";
import type { StepDTO } from "../store/protocol";
import { microtaskAffordance } from "./microtask-affordance";

function step(overrides: Partial<StepDTO> = {}): StepDTO {
  return {
    id: "step-1",
    itemId: "item-1",
    body: "put on music",
    done: false,
    position: 1,
    deletedAt: null,
    version: 1,
    ...overrides,
  };
}

describe("microtaskAffordance", () => {
  it("an item with no steps offers break", () => {
    expect(microtaskAffordance([])).toEqual({ kind: "break" });
  });

  /** #307 point 1: ticked steps are record, not plan — an append after them
   * is the normal case, and "Rewrite 0 steps" would both read as nonsense
   * and send a `replace: true` with nothing to replace. */
  it("an item whose live steps are ALL done offers break, not rewrite", () => {
    expect(microtaskAffordance([step({ id: "a", done: true }), step({ id: "b", done: true })])).toEqual({
      kind: "break",
    });
  });

  it("an item with only soft-deleted steps offers break", () => {
    expect(microtaskAffordance([step({ deletedAt: 1_000 })])).toEqual({ kind: "break" });
  });

  it("a live undone plan offers rewrite, counting only the undone", () => {
    const steps = [
      step({ id: "a", done: true }),
      step({ id: "b" }),
      step({ id: "c" }),
      step({ id: "d", deletedAt: 1_000 }),
    ];
    expect(microtaskAffordance(steps)).toEqual({ kind: "rewrite", undoneCount: 2 });
  });
});
