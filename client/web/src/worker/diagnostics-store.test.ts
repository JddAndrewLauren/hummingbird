import { IDBFactory, IDBKeyRange } from "fake-indexeddb";
import { describe, expect, it, vi } from "vitest";
import type { DiagnosticEventV1DTO } from "../store/protocol";
import {
  createDiagnosticsStore,
  DIAGNOSTICS_MAX_BYTES,
  DIAGNOSTICS_RETENTION_MS,
  openDiagnosticsDb,
} from "./diagnostics-store";
import { planRetention, type RetentionRecord } from "./diagnostics-retention";

// Node has no ambient `IDBKeyRange`; the store's age-eviction sweep needs
// one to build a bounded index range, so every call below hands it
// `fake-indexeddb`'s own real implementation, the same injection idiom the
// `IDBFactory` argument already uses.
function newStore(factory: IDBFactory = new IDBFactory()) {
  return createDiagnosticsStore(factory, IDBKeyRange);
}

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
    const store = newStore();
    const result = await store.exportAll();
    expect(result).toEqual({ events: [], droppedCount: 0 });
  });

  it("exports written events in insertion (sequence) order", async () => {
    const store = newStore();
    await store.append([event({ seq: 1 }), event({ seq: 2 })], 1_000);
    await store.append([event({ seq: 3 })], 1_001);

    const result = await store.exportAll();
    expect(result.events.map((e) => e.seq)).toEqual([1, 2, 3]);
  });

  it("clear empties the journal and resets the dropped count", async () => {
    const store = newStore();
    await store.append([event()], 1_000);

    await store.clear();

    const result = await store.exportAll();
    expect(result).toEqual({ events: [], droppedCount: 0 });
  });

  it("survives a simulated worker restart: a second store over the same factory sees events written before it existed", async () => {
    const factory = new IDBFactory();
    const firstLifetime = newStore(factory);
    await firstLifetime.append([event({ seq: 1 }), event({ seq: 2 })], 1_000);

    // A new `DiagnosticsStoreLike` instance, exactly what a fresh
    // `SharedWorker` activation constructs — nothing here reuses the first
    // instance's in-memory state, only the underlying (fake) IndexedDB.
    const secondLifetime = newStore(factory);
    const result = await secondLifetime.exportAll();

    expect(result.events.map((e) => e.seq)).toEqual([1, 2]);
  });

  it("preserves sequence order across a restart boundary", async () => {
    const factory = new IDBFactory();
    await newStore(factory).append([event({ seq: 1 })], 1_000);
    // Simulated restart, then more events append after it.
    await newStore(factory).append([event({ seq: 2 })], 1_001);

    const result = await newStore(factory).exportAll();
    expect(result.events.map((e) => e.seq)).toEqual([1, 2]);
  });

  // Review round 1 of PR #736: a fresh `IDBDatabase` connection per
  // operation, never reused, is a real leak — this pins the fix.
  it("opens exactly one connection for the store's whole lifetime, across many operations", async () => {
    const factory = new IDBFactory();
    const openSpy = vi.spyOn(factory, "open");
    const store = newStore(factory);

    await store.append([event({ seq: 1 })], 1_000);
    await store.append([event({ seq: 2 })], 1_000);
    await store.exportAll();
    await store.clear();

    expect(openSpy).toHaveBeenCalledTimes(1);
  });

  // Review round 1: `append` used to read the WHOLE store and
  // `JSON.stringify` every record on every call — i.e. every 250ms during
  // exactly the stall this journal exists to observe. The age sweep is an
  // indexed range query (`evictExpired`), not a full scan, so it opens a
  // cursor on the `wall_clock_ms` INDEX rather than on the object store
  // directly when nothing is expired; this pins that the object store's
  // own (unbounded) cursor is never opened by an append that has nothing
  // to evict on either bound.
  it("never opens a full object-store cursor on an append that evicts nothing on either bound", async () => {
    const factory = new IDBFactory();
    const store = newStore(factory);
    await store.append([event({ seq: 1 })], 1_000);

    const db = await openDiagnosticsDb(factory);
    const tx = db.transaction(["events"], "readonly");
    const objectStoreOpenCursor = vi.spyOn(Object.getPrototypeOf(tx.objectStore("events")), "openCursor");
    db.close();

    await store.append([event({ seq: 2 })], 1_000);

    expect(objectStoreOpenCursor).not.toHaveBeenCalled();
    objectStoreOpenCursor.mockRestore();
  });

  describe("retention", () => {
    it("drops events older than 72 hours by the caller's injected clock, and counts them", async () => {
      const store = newStore();
      const oldMs = 0;
      const nowMs = DIAGNOSTICS_RETENTION_MS + 10_000;

      await store.append([event({ seq: 1, wall_clock_ms: oldMs })], oldMs);
      await store.append([event({ seq: 2, wall_clock_ms: nowMs })], nowMs);

      const result = await store.exportAll();
      expect(result.events.map((e) => e.seq)).toEqual([2]);
      expect(result.droppedCount).toBe(1);
    });

    it("drops the oldest events first once the 10 MiB bound is crossed, and counts them", async () => {
      const store = newStore();
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
      const store = newStore();
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

    // Review round 2 of PR #736: both `diagnostics-retention.ts` and this
    // module's own doc claimed "`diagnostics-store.test.ts` proves the two
    // agree on the same observable outcomes" — and no test in this file
    // imported `planRetention` at all. There was no differential test; the
    // claim was false. This is it.
    //
    // `planRetention` has NO runtime caller (the store realises the same
    // policy over indexed cursors instead, for the IO reason both module
    // docs state). What keeps it from being dead code is exactly this
    // test: it is the independent ORACLE the store's cursor
    // implementation is checked against. Delete `planRetention` and this
    // test stops compiling — which is the property that makes the two
    // module docs' cross-references true rather than aspirational.
    //
    // The comparison is exact, not "both evicted something": one `append`
    // of the whole batch puts the store through age-then-size in a single
    // transaction, which is precisely the order `planRetention` plans in,
    // so the two are directly comparable on the KEY SET — not merely on a
    // count or a surviving-byte bound (the two assertions above already
    // cover those weaker properties).
    it("evicts exactly the keys planRetention plans — the store's cursor sweeps against the pure policy oracle", async () => {
      const store = newStore();
      const nowMs = DIAGNOSTICS_RETENTION_MS + 100_000;
      // 2.5 MiB each: two already expired (age sweep), five fresh totalling
      // 12.5 MiB against the 10 MiB cap (size sweep). Both bounds fire in
      // the same append, which is what makes this a real differential over
      // the combined policy rather than over one bound at a time.
      const pad = "x".repeat(2.5 * 1024 * 1024);
      const events = [
        event({ seq: 1, wall_clock_ms: 0, request_id: pad }),
        event({ seq: 2, wall_clock_ms: 0, request_id: pad }),
        event({ seq: 3, wall_clock_ms: nowMs, request_id: pad }),
        event({ seq: 4, wall_clock_ms: nowMs, request_id: pad }),
        event({ seq: 5, wall_clock_ms: nowMs, request_id: pad }),
        event({ seq: 6, wall_clock_ms: nowMs, request_id: pad }),
        event({ seq: 7, wall_clock_ms: nowMs, request_id: pad }),
      ];

      // The `events` object store is `autoIncrement` from 1, so the Nth
      // event appended holds primary key N — that is what lets a surviving
      // `seq` below be read back as a surviving KEY.
      const records: RetentionRecord[] = events.map((e, index) => ({
        key: index + 1,
        wallClockMs: e.wall_clock_ms,
        byteLength: new TextEncoder().encode(JSON.stringify(e)).length,
      }));
      const plan = planRetention(records, nowMs, {
        retentionMs: DIAGNOSTICS_RETENTION_MS,
        maxBytes: DIAGNOSTICS_MAX_BYTES,
      });

      // Guards the differential against being vacuously green: the plan
      // must actually evict on BOTH bounds, or the key-set comparison
      // below would pass over a policy that did nothing.
      expect(plan.evictKeys).toContain(1);
      expect(plan.evictKeys).toContain(2);
      expect(plan.evictKeys.length).toBeGreaterThan(2);

      await store.append(events, nowMs);

      const result = await store.exportAll();
      const survivingKeys = result.events.map((e) => e.seq);
      const expectedSurvivors = records
        .map((r) => r.key)
        .filter((key) => !plan.evictKeys.includes(key));

      expect(survivingKeys).toEqual(expectedSurvivors);
      expect(result.droppedCount).toBe(plan.droppedCount);
    });
  });
});
