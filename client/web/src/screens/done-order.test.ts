import { describe, expect, it } from "vitest";
import type { TaskItemDTO } from "../store/protocol";
import { orderDone } from "./done-order";

function item(id: string, updatedAt: number): TaskItemDTO {
  return {
    id,
    seq: 1,
    title: `item ${id}`,
    description: null,
    stage: "done",
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
    linkUrl: null,
    linkLabel: null,
    archivedAt: null,
    createdAt: 1_000,
    updatedAt,
    version: 1,
    pending: false,
  };
}

describe("orderDone", () => {
  it("orders most recently touched first, id ascending on ties, without mutating", () => {
    const items = [item("b", 1_000), item("a", 1_000), item("c", 4_000)];
    const before = [...items];

    expect(orderDone(items).map((i) => i.id)).toEqual(["c", "a", "b"]);
    expect(items).toEqual(before);
  });
});
