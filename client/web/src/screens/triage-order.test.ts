import { describe, expect, it } from "vitest";
import type { TaskItemDTO } from "../store/protocol";
import { orderTriage } from "./triage-order";

// Issue #110/S12 acceptance: "Offline, three captures then reconnecting
// produces three Triage items in order, with no duplicates." The
// no-duplicates half is `client/core`'s deterministic-id guarantee
// (`three_offline_captures_then_reconnecting_produce_three_distinct_triage_items`);
// this is the display-order half.

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

describe("orderTriage", () => {
  it("is a pure function: the input array is never mutated", () => {
    const input = [item({ id: "a", createdAt: 2 }), item({ id: "b", createdAt: 1 })];
    const copy = [...input];

    orderTriage(input);

    expect(input).toEqual(copy);
  });

  it("orders three offline captures by capture time, oldest first", () => {
    const third = item({ id: "c", createdAt: 3_000 });
    const first = item({ id: "a", createdAt: 1_000 });
    const second = item({ id: "b", createdAt: 2_000 });

    expect(orderTriage([third, first, second]).map((i) => i.id)).toEqual(["a", "b", "c"]);
  });

  it("breaks a tie on createdAt by id, for a stable order", () => {
    const b = item({ id: "b", createdAt: 1_000 });
    const a = item({ id: "a", createdAt: 1_000 });

    expect(orderTriage([b, a]).map((i) => i.id)).toEqual(["a", "b"]);
  });

  it("reading it twice with the same input yields the same output", () => {
    const input = [item({ id: "b", createdAt: 2 }), item({ id: "a", createdAt: 1 })];

    expect(orderTriage(input)).toEqual(orderTriage(input));
  });
});
