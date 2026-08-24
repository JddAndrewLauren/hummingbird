import { IDBFactory } from "fake-indexeddb";
import { describe, expect, it } from "vitest";
import type { DiagnosticEventV1DTO } from "../store/protocol";
import { createDiagnosticsStore, DIAGNOSTICS_MAX_BYTES, DIAGNOSTICS_RETENTION_MS } from "./diagnostics-store";

// `fake-indexeddb`'s `IDBFactory` is a real, spec-conformant in-memory
// implementation (not a mock of this module's own calls), so these tests
// exercise the actual `IDBTransaction`/`IDBObjectStore`/cursor machinery —
// a fresh `IDBFactory` per test isolates one test's database from another's
// (a real browser origin would not, but nothing here needs that).

function event(overrides: Partial<DiagnosticEventV1DTO> = {}): DiagnosticEventV1DTO {
  return {
    schema_version: 1,
    seq: 1,
    wall_clock_ms: 1_000,
    elapsed_ms: 0,
    session_id: "s-1",
    source: "web-worker",
    cycle_id: null,
    operation_id: null,
    request_id: null,
    event: { name: "core.wait_started" },
    ...overrides,
  };
}

describe("createDiagnosticsStore", () => {
  it("exports nothing and a zero dropped count from a fresh journal", async () => {
    const store = createDiagnosticsStore(new IDBFactory());
    const result = await store.exportAll();
    expect(result).toEqual({ events: [], droppedCount: 0 });
  });

  it("exports written events in insertion (sequence) order", async () => {
    const store = createDiagnosticsStore(new IDBFactory());
    await store.append([event({ seq: 1 }), event({ seq: 2 })], 1_000);
    await store.append([event({ seq: 3 })], 1_001);

    const result = await store.exportAll();
    expect(result.events.map((e) => e.seq)).toEqual([1, 2, 3]);
  });

  it("clear empties the journal and resets the dropped count", async () => {
    const store = createDiagnosticsStore(new IDBFactory());
    await store.append([event()], 1_000);

    await store.clear();

    const result = await store.exportAll();
    expect(result).toEqual({ events: [], droppedCount: 0 });
  });

  it("survives a simulated worker restart: a second store over the same factory sees events written before it existed", async () => {
    const factory = new IDBFactory();
    const firstLifetime = createDiagnosticsStore(factory);
    await firstLifetime.append([event({ seq: 1 }), event({ seq: 2 })], 1_000);

    // A new `DiagnosticsStoreLike` instance, exactly what a fresh
    // `SharedWorker` activation constructs — nothing here reuses the first
    // instance's in-memory state, only the underlying (fake) IndexedDB.
    const secondLifetime = createDiagnosticsStore(factory);
    const result = await secondLifetime.exportAll();

    expect(result.events.map((e) => e.seq)).toEqual([1, 2]);
  });

  it("preserves sequence order across a restart boundary", async () => {
    const factory = new IDBFactory();
    await createDiagnosticsStore(factory).append([event({ seq: 1 })], 1_000);
    // Simulated restart, then more events append after it.
    await createDiagnosticsStore(factory).append([event({ seq: 2 })], 1_001);

    const result = await createDiagnosticsStore(factory).exportAll();
    expect(result.events.map((e) => e.seq)).toEqual([1, 2]);
  });

  describe("retention", () => {
    it("drops events older than 72 hours by the caller's injected clock, and counts them", async () => {
      const store = createDiagnosticsStore(new IDBFactory());
      const oldMs = 0;
      const nowMs = DIAGNOSTICS_RETENTION_MS + 10_000;

      await store.append([event({ seq: 1, wall_clock_ms: oldMs })], oldMs);
      await store.append([event({ seq: 2, wall_clock_ms: nowMs })], nowMs);

      const result = await store.exportAll();
      expect(result.events.map((e) => e.seq)).toEqual([2]);
      expect(result.droppedCount).toBe(1);
    });

    it("drops the oldest events first once the 10 MiB bound is crossed, and counts them", async () => {
      const store = createDiagnosticsStore(new IDBFactory());
      // Each padded event serializes to well over 3 MiB; four of them cross
      // the 10 MiB bound and force at least one eviction.
      const pad = "x".repeat(3.5 * 1024 * 1024);
      const big = (seq: number) => event({ seq, request_id: pad });

      await store.append([big(1)], 1_000);
      await store.append([big(2)], 1_000);
      await store.append([big(3)], 1_000);
      await store.append([big(4)], 1_000);

      const result = await store.exportAll();
      expect(result.events.length).toBeLessThan(4);
      expect(result.droppedCount).toBeGreaterThan(0);
      // The survivors are the most recently written (highest seq) —
      // eviction always removes the oldest first.
      const seqs = result.events.map((e) => e.seq);
      expect(seqs).toEqual([...seqs].sort((a, b) => a - b));
      expect(Math.max(...seqs)).toBe(4);
      expect(seqs).not.toContain(1);
    });

    it("never lets the journal exceed the 10 MiB bound", async () => {
      const store = createDiagnosticsStore(new IDBFactory());
      const pad = "x".repeat(3.5 * 1024 * 1024);
      for (let seq = 1; seq <= 5; seq += 1) {
        await store.append([event({ seq, request_id: pad })], 1_000);
      }

      const result = await store.exportAll();
      const totalBytes = result.events.reduce(
        (sum, e) => sum + new TextEncoder().encode(JSON.stringify(e)).length,
        0,
      );
      expect(totalBytes).toBeLessThanOrEqual(DIAGNOSTICS_MAX_BYTES);
    });
  });
});
