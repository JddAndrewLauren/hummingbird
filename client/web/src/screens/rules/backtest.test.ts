import { describe, expect, it } from "vitest";
import type { TaskItemDTO } from "../../store/protocol";
import { backtest } from "./backtest";

function item(overrides: Partial<TaskItemDTO> = {}): TaskItemDTO {
  return {
    id: "i-1",
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
    createdAt: 1,
    updatedAt: 1,
    version: 1,
    pending: false,
    ...overrides,
  };
}

const NOW = new Date("2026-08-15T12:00:00Z").getTime();

describe("backtest", () => {
  it("reports unavailable for a kind other than item_threshold", () => {
    const result = backtest({ eventKind: "email", conditions: [] }, [item()], NOW);
    expect(result).toEqual({ kind: "unavailable", reason: "no_local_history" });
  });

  it("is ok, not unavailable, for eventKind null (\"any kind\") — item_threshold still satisfies it", () => {
    const matching = item({ id: "i-1", title: "renew passport" });
    const result = backtest(
      { eventKind: null, conditions: [{ field: "title", op: "contains", value: "passport", negate: false }] },
      [matching],
      NOW,
    );
    expect(result.kind).toBe("ok");
  });

  it("calendar_busy is always false — the server's own synthesis never sets it", () => {
    const anItem = item({ id: "i-1" });
    const busyMatch = backtest(
      { eventKind: "item_threshold", conditions: [{ field: "calendar_busy", op: "is", value: true, negate: false }] },
      [anItem],
      NOW,
    );
    const notBusyMatch = backtest(
      { eventKind: "item_threshold", conditions: [{ field: "calendar_busy", op: "is", value: false, negate: false }] },
      [anItem],
      NOW,
    );
    expect(busyMatch.kind === "ok" ? busyMatch.matches : []).toEqual([]);
    expect(notBusyMatch.kind === "ok" ? notBusyMatch.matches.map((i) => i.id) : []).toEqual(["i-1"]);
  });

  it("occurred_at is derived from updatedAt, exactly matching now_as_deadline's UTC minute truncation", () => {
    // updatedAt = 2026-08-15T11:00:30Z -> occurred_at "2026-08-15T11:00" (seconds truncated, UTC).
    const recentlyUpdated = item({ id: "i-1", updatedAt: Date.UTC(2026, 7, 15, 11, 0, 30) });
    const result = backtest(
      {
        eventKind: "item_threshold",
        conditions: [{ field: "occurred_at", op: "within_last", value: "2h", negate: false }],
      },
      [recentlyUpdated],
      NOW, // 2026-08-15T12:00:00Z
    );
    expect(result.kind === "ok" ? result.matches.map((i) => i.id) : []).toEqual(["i-1"]);
  });

  it("matches item_threshold conditions against the mirrored item list", () => {
    const matching = item({ id: "i-1", title: "renew passport" });
    const other = item({ id: "i-2", title: "buy milk" });
    const result = backtest(
      {
        eventKind: "item_threshold",
        conditions: [{ field: "title", op: "contains", value: "passport", negate: false }],
      },
      [matching, other],
      NOW,
    );
    expect(result.kind).toBe("ok");
    expect(result.kind === "ok" ? result.matches.map((i) => i.id) : []).toEqual(["i-1"]);
  });

  it("ANDs multiple conditions", () => {
    const matches = item({ id: "i-1", title: "renew passport", stage: "ready" });
    const wrongStage = item({ id: "i-2", title: "renew passport", stage: "blocked" });
    const result = backtest(
      {
        eventKind: "item_threshold",
        conditions: [
          { field: "title", op: "contains", value: "passport", negate: false },
          { field: "stage", op: "eq", value: "ready", negate: false },
        ],
      },
      [matches, wrongStage],
      NOW,
    );
    expect(result.kind === "ok" ? result.matches.map((i) => i.id) : []).toEqual(["i-1"]);
  });

  it("respects negate, and a missing field is false even negated", () => {
    const withProject = item({ id: "i-1", projectId: "p-1" });
    const withoutProject = item({ id: "i-2", projectId: null });
    const result = backtest(
      { eventKind: "item_threshold", conditions: [{ field: "project", op: "eq", value: "p-1", negate: true }] },
      [withProject, withoutProject],
      NOW,
    );
    // i-1: project eq p-1 is true, negated -> false, excluded.
    // i-2: project field missing -> condition false regardless of negate, excluded.
    expect(result.kind === "ok" ? result.matches : []).toEqual([]);
  });

  it("evaluates within_next against a deadline, unbounded on the past side", () => {
    const overdue = item({ id: "i-1", deadline: "2026-08-10T09:00Z" }); // in the past
    const soon = item({ id: "i-2", deadline: "2026-08-15T13:00Z" }); // 1h from NOW
    const later = item({ id: "i-3", deadline: "2026-08-20T09:00Z" });
    const result = backtest(
      {
        eventKind: "item_threshold",
        conditions: [{ field: "deadline", op: "within_next", value: "2h", negate: false }],
      },
      [overdue, soon, later],
      NOW,
    );
    expect(result.kind === "ok" ? result.matches.map((i) => i.id).sort() : []).toEqual(["i-1", "i-2"]);
  });

  it("writes nothing and calls nothing — a pure function over its arguments", () => {
    const before = item({ id: "i-1" });
    backtest({ eventKind: "item_threshold", conditions: [] }, [before], NOW);
    expect(before).toEqual(item({ id: "i-1" }));
  });
});
