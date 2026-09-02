import { describe, expect, it } from "vitest";
import type { LedgerRowDTO } from "../store/protocol";
import { lastTouchedMs, ledgerRowState, orderLedger } from "./ledger-order";

function row(overrides: Partial<LedgerRowDTO> = {}): LedgerRowDTO {
  return {
    id: "item-1",
    seq: 1,
    title: "An action",
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
    vaultPath: null,
    archivedAt: null,
    createdAt: 1_000,
    updatedAt: 1_000,
    version: 1,
    pending: false,
    absentSinceMs: null,
    deadLettered: false,
    hasLiveAlert: false,
    ...overrides,
  };
}

describe("ledgerRowState", () => {
  it("a live row is live", () => {
    expect(ledgerRowState(row())).toEqual({ kind: "live" });
  });

  it("an explicitly archived row is archived as of its own flag", () => {
    expect(ledgerRowState(row({ archivedAt: 5_000, absentSinceMs: 6_000 }))).toEqual({
      kind: "archived",
      sinceMs: 5_000,
    });
  });

  it("a row demoted by a sweep with no flag is archived as of the demotion stamp", () => {
    expect(ledgerRowState(row({ absentSinceMs: 7_000 }))).toEqual({
      kind: "archived",
      sinceMs: 7_000,
    });
  });
});

describe("lastTouchedMs", () => {
  it("is updatedAt for a live row", () => {
    expect(lastTouchedMs(row({ updatedAt: 3_000 }))).toBe(3_000);
  });

  it("an archive or demotion stamp later than updatedAt wins — archiving is a flag write", () => {
    expect(lastTouchedMs(row({ updatedAt: 3_000, archivedAt: 9_000 }))).toBe(9_000);
    expect(lastTouchedMs(row({ updatedAt: 3_000, absentSinceMs: 8_000 }))).toBe(8_000);
  });
});

describe("orderLedger", () => {
  it("orders last touched first, id ascending on ties, without mutating its input", () => {
    const rows = [
      row({ id: "b", updatedAt: 1_000 }),
      row({ id: "a", updatedAt: 1_000 }),
      row({ id: "c", updatedAt: 5_000 }),
      row({ id: "d", updatedAt: 2_000, archivedAt: 9_000 }),
    ];
    const before = [...rows];

    expect(orderLedger(rows).map((r) => r.id)).toEqual(["d", "c", "a", "b"]);
    expect(rows).toEqual(before);
  });
});
