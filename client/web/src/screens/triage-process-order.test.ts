import { describe, expect, it } from "vitest";
import type { TaskItemDTO } from "../store/protocol";
import { triageProcessQueue } from "./triage-process-order";

// Issue #357 acceptance: one pure function owns the combined "triage
// process" (CONTEXT.md) queue's membership AND order — drafts first, then
// Grilling-stage items, then captured Triage items — so Triage and Now's
// collapsible area render from the same fact instead of each inferring it.

function item(overrides: Partial<TaskItemDTO> = {}): TaskItemDTO {
  return {
    id: "id-0",
    seq: null,
    title: "untitled",
    description: null,
    stage: "triage",
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

describe("triageProcessQueue", () => {
  it("is a pure function: neither input array is mutated", () => {
    const triage = [item({ id: "a", stage: "triage" })];
    const grilling = [item({ id: "b", stage: "grilling" })];
    const triageCopy = [...triage];
    const grillingCopy = [...grilling];

    triageProcessQueue(triage, grilling, []);

    expect(triage).toEqual(triageCopy);
    expect(grilling).toEqual(grillingCopy);
  });

  it("orders local drafts first, then Grilling, then captured Triage", () => {
    const captured = item({ id: "c", stage: "triage", createdAt: 1_000 });
    const drafted = item({ id: "d", stage: "triage", createdAt: 2_000 });
    const grilling = item({ id: "g", stage: "grilling", createdAt: 3_000 });

    const queue = triageProcessQueue([captured, drafted], [grilling], ["d"]);

    expect(queue.items.map((i) => i.id)).toEqual(["d", "g", "c"]);
  });

  it("preserves orderTriage's oldest-capture-first order within the captured group", () => {
    const second = item({ id: "b", stage: "triage", createdAt: 2_000 });
    const first = item({ id: "a", stage: "triage", createdAt: 1_000 });

    const queue = triageProcessQueue([second, first], [], []);

    expect(queue.items.map((i) => i.id)).toEqual(["a", "b"]);
  });

  it("counts are exact captured/grilling totals, never a single sum", () => {
    const captured = [item({ id: "a" }), item({ id: "b" })];
    const grilling = [item({ id: "c", stage: "grilling" })];

    const queue = triageProcessQueue(captured, grilling, []);

    expect(queue.capturedCount).toBe(2);
    expect(queue.grillingCount).toBe(1);
  });

  it("a draft still counts toward its own stage's total, not a third bucket", () => {
    const drafted = item({ id: "a", stage: "triage" });

    const queue = triageProcessQueue([drafted], [], ["a"]);

    expect(queue.capturedCount).toBe(1);
    expect(queue.grillingCount).toBe(0);
    expect(queue.items.map((i) => i.id)).toEqual(["a"]);
  });

  it("a device with no drafts and no Grilling items renders exactly orderTriage's own order", () => {
    const second = item({ id: "b", createdAt: 2_000 });
    const first = item({ id: "a", createdAt: 1_000 });

    const queue = triageProcessQueue([second, first], [], []);

    expect(queue.items.map((i) => i.id)).toEqual(["a", "b"]);
    expect(queue.capturedCount).toBe(2);
    expect(queue.grillingCount).toBe(0);
  });

  it("reading it twice with the same input yields the same output", () => {
    const triage = [item({ id: "b", createdAt: 2 }), item({ id: "a", createdAt: 1 })];
    const grilling = [item({ id: "c", stage: "grilling", createdAt: 3 })];

    expect(triageProcessQueue(triage, grilling, ["a"])).toEqual(
      triageProcessQueue(triage, grilling, ["a"]),
    );
  });
});
