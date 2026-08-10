import { describe, expect, it } from "vitest";
import type { TaskItemDTO } from "../store/protocol";
import { orderFrontier } from "./frontier-order";

// Acceptance criterion (issue #108): "Ordering is a pure function and is
// unit-tested." — `orderFrontier` never mutates its input and produces the
// same output for the same input every time; nothing here reads a clock or
// any other ambient state.

function item(overrides: Partial<TaskItemDTO> = {}): TaskItemDTO {
  return {
    id: "id-0",
    seq: null,
    title: "untitled",
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
    createdAt: 1,
    updatedAt: 1,
    version: 0,
    pending: false,
    ...overrides,
  };
}

describe("orderFrontier", () => {
  it("is a pure function: the input array is never mutated", () => {
    const input = [item({ id: "a", priority: 0 }), item({ id: "b", priority: 1 })];
    const copy = [...input];

    orderFrontier(input);

    expect(input).toEqual(copy);
  });

  it("returns the same order for the same input every call", () => {
    const input = [item({ id: "a", priority: 3 }), item({ id: "b", priority: 1 })];

    expect(orderFrontier(input)).toEqual(orderFrontier(input));
  });

  it("ranks by priority label, never the raw (inverted, holed) wire number", () => {
    const none = item({ id: "none", priority: 0 });
    const urgent = item({ id: "urgent", priority: 1 });
    const low = item({ id: "low", priority: 4 });

    const ordered = orderFrontier([none, low, urgent]).map((i) => i.id);

    expect(ordered).toEqual(["urgent", "low", "none"]);
  });

  it("within the same priority, orders by deadline chronologically", () => {
    const soon = item({ id: "soon", priority: 1, deadline: "2026-08-15" });
    const later = item({ id: "later", priority: 1, deadline: "2026-08-20" });
    const none = item({ id: "none-deadline", priority: 1, deadline: null });

    const ordered = orderFrontier([none, later, soon]).map((i) => i.id);

    expect(ordered).toEqual(["soon", "later", "none-deadline"]);
  });

  it("a day-only deadline sorts after an explicit same-day time (end-of-day resolution)", () => {
    const dayOnly = item({ id: "day-only", priority: 1, deadline: "2026-08-15" });
    const explicitLate = item({ id: "explicit-late", priority: 1, deadline: "2026-08-15T18:00" });

    const ordered = orderFrontier([dayOnly, explicitLate]).map((i) => i.id);

    expect(ordered).toEqual(["explicit-late", "day-only"]);
  });

  it("falls back to id for a fully-tied ordering, for a stable display order", () => {
    const a = item({ id: "a", priority: 2 });
    const b = item({ id: "b", priority: 2 });

    expect(orderFrontier([b, a]).map((i) => i.id)).toEqual(["a", "b"]);
  });
});
