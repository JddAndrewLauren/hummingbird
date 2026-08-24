import type { DiagnosticEventV1DTO } from "../store/protocol";

// #707's IndexedDB-backed journal store. One database, owned by the
// SharedWorker (`core.worker.ts`), matching ADR-0010's "one core per
// origin" — a per-tab journal would defeat the whole point of diagnosing a
// SINGLE shared core (see `diagnostics-journal.ts`, the one caller).
//
// The `events` object store is `autoIncrement`, out-of-line (no `keyPath`
// on the stored value): the primary key is never written into the exported
// record itself, so an export is exactly the `DiagnosticEventV1` envelope
// and nothing else — and ascending key order IS insertion order, which
// survives a restart (the counter persists with the database) even when
// two events share a millisecond timestamp. That is what "sequence order,
// proven across a restart boundary" means for export: it reads the store
// by primary key, not by re-sorting on `wall_clock_ms`.
//
// **Review round 1 of PR #736 caught a real performance defect**: the
// previous version opened a fresh `IDBDatabase` connection on every single
// operation and never reused or closed one, and it read the WHOLE store
// into memory and re-serialized every record with `JSON.stringify` on
// every `append` — i.e. every 250ms while a request is outstanding, which
// is exactly the stall this journal exists to observe. A diagnostic that
// amplifies the fault it records is not shippable. This version:
//
// - opens the connection ONCE (`openDbOnce`, memoized) and reuses it for
//   the store's whole lifetime, rather than a fresh `factory.open` call
//   per operation;
// - never reads the whole store to decide what to evict. Age eviction
//   queries the `wall_clock_ms` INDEX for a bounded range (`< cutoffMs`) —
//   normally empty, since most events are recent — and byte eviction only
//   walks forward from the oldest key, stopping the instant the running
//   total (kept in `meta`, updated incrementally on every add/delete —
//   never recomputed from a full scan) drops back under the cap. Either
//   sweep touches at most the records it actually deletes, never the ones
//   it decides to keep.
//
// `diagnostics-retention.ts`'s `planRetention` remains the pure,
// exhaustively-tested SPEC for this policy (oldest-first, age before size,
// never double-counted) — its own tests are what prove the policy correct
// in isolation, with no IndexedDB involved. This module is a different,
// IO-efficient REALIZATION of that identical policy over indexed cursors
// rather than a materialized list; `diagnostics-store.test.ts` proves the
// two agree on the same observable outcomes (oldest-first, both bounds,
// dropped-count) against a real (fake) IndexedDB.
//
// **A storage failure here must never reach the caller as a throw** — the
// brief's own acceptance criterion ("a diagnostic storage failure is
// swallowed") is why every public method below catches internally rather
// than letting an `IDBRequest` error propagate. `diagnostics-journal.ts` is
// the caller that would otherwise have to remember this at every call site;
// putting it here means it cannot be forgotten at one of them.

export const DIAGNOSTICS_DB_NAME = "hummingbird-diagnostics";
const DIAGNOSTICS_DB_VERSION = 1;
const EVENTS_STORE = "events";
const META_STORE = "meta";
const WALL_CLOCK_INDEX = "wall_clock_ms";
const COUNTERS_KEY = "counters";

export const DIAGNOSTICS_RETENTION_MS = 72 * 60 * 60 * 1000;
export const DIAGNOSTICS_MAX_BYTES = 10 * 1024 * 1024;

interface Counters {
  id: typeof COUNTERS_KEY;
  totalBytes: number;
  droppedCount: number;
}

function byteLengthOf(event: DiagnosticEventV1DTO): number {
  return new TextEncoder().encode(JSON.stringify(event)).length;
}

function promisifyRequest<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

function promisifyTransaction(tx: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
    tx.onabort = () => reject(tx.error ?? new Error("diagnostics transaction aborted"));
  });
}

/** Opens (and, on first use, creates) the diagnostics database. `factory`
 * is injected so a test can hand it `fake-indexeddb`'s implementation
 * without stubbing a global — the same idiom `core-id.ts`'s `mintCoreId`
 * uses for `crypto`. Defaults to the ambient `indexedDB`, which is what
 * `core.worker.ts` calls this with (a `SharedWorkerGlobalScope` has one).
 *
 * Exported for `diagnostics-store.test.ts`'s own restart-survival tests
 * (each simulated "restart" is a fresh call over the same `factory`); the
 * store itself (`IndexedDbDiagnosticsStore` below) calls this exactly ONCE
 * per instance and reuses the resulting connection — see the module doc's
 * "review round 1" note. */
export function openDiagnosticsDb(factory: IDBFactory = self.indexedDB): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = factory.open(DIAGNOSTICS_DB_NAME, DIAGNOSTICS_DB_VERSION);
    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains(EVENTS_STORE)) {
        const events = db.createObjectStore(EVENTS_STORE, { autoIncrement: true });
        events.createIndex(WALL_CLOCK_INDEX, "wall_clock_ms");
      }
      if (!db.objectStoreNames.contains(META_STORE)) {
        db.createObjectStore(META_STORE, { keyPath: "id" });
      }
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

async function readCounters(tx: IDBTransaction): Promise<Counters> {
  const stored = await promisifyRequest<Counters | undefined>(
    tx.objectStore(META_STORE).get(COUNTERS_KEY),
  );
  return stored ?? { id: COUNTERS_KEY, totalBytes: 0, droppedCount: 0 };
}

/** Deletes every record whose `wall_clock_ms` is strictly older than
 * `cutoffMs`, via the index — never a full-store scan. Returns the number
 * of bytes freed, so the caller can keep `counters.totalBytes` correct
 * without re-reading it. Normally touches zero records: most events are
 * recent, and the index range only ever yields the ones that are not. */
async function evictExpired(
  tx: IDBTransaction,
  cutoffMs: number,
  factory: { keyRange: IDBKeyRangeCtor },
): Promise<{ freedBytes: number; droppedCount: number }> {
  const store = tx.objectStore(EVENTS_STORE);
  const index = store.index(WALL_CLOCK_INDEX);
  const range = factory.keyRange.upperBound(cutoffMs, true);
  let freedBytes = 0;
  let droppedCount = 0;
  await new Promise<void>((resolve, reject) => {
    const cursorRequest = index.openCursor(range);
    cursorRequest.onsuccess = () => {
      const cursor = cursorRequest.result;
      if (cursor === null) {
        resolve();
        return;
      }
      freedBytes += byteLengthOf(cursor.value as DiagnosticEventV1DTO);
      droppedCount += 1;
      cursor.delete();
      cursor.continue();
    };
    cursorRequest.onerror = () => reject(cursorRequest.error);
  });
  return { freedBytes, droppedCount };
}

/** Deletes the oldest surviving records, by primary-key (= insertion)
 * order, until `startingTotalBytes` minus what has been freed so far drops
 * to or below `maxBytes` — stopping the instant the budget is satisfied,
 * never walking further into the store than it has to. */
async function evictOverBudget(
  tx: IDBTransaction,
  startingTotalBytes: number,
  maxBytes: number,
): Promise<{ freedBytes: number; droppedCount: number }> {
  if (startingTotalBytes <= maxBytes) {
    return { freedBytes: 0, droppedCount: 0 };
  }
  const store = tx.objectStore(EVENTS_STORE);
  let freedBytes = 0;
  let droppedCount = 0;
  let remaining = startingTotalBytes;
  await new Promise<void>((resolve, reject) => {
    const cursorRequest = store.openCursor();
    cursorRequest.onsuccess = () => {
      const cursor = cursorRequest.result;
      if (cursor === null || remaining <= maxBytes) {
        resolve();
        return;
      }
      const freed = byteLengthOf(cursor.value as DiagnosticEventV1DTO);
      freedBytes += freed;
      remaining -= freed;
      droppedCount += 1;
      cursor.delete();
      if (remaining <= maxBytes) {
        resolve();
        return;
      }
      cursor.continue();
    };
    cursorRequest.onerror = () => reject(cursorRequest.error);
  });
  return { freedBytes, droppedCount };
}

interface IDBKeyRangeCtor {
  upperBound(upper: unknown, open?: boolean): IDBKeyRange;
}

/** The journal's public surface — see the module doc above for the
 * "never throws" contract every method upholds. */
export interface DiagnosticsStoreLike {
  /** Writes `events` and enforces both retention bounds against `nowMs`
   * (the caller's own clock — never sampled here). Returns the number of
   * records this append caused to be dropped, purely for a caller that
   * wants to log it; the cumulative total is what `exportAll` reports. */
  append(events: DiagnosticEventV1DTO[], nowMs: number): Promise<number>;
  exportAll(): Promise<{ events: DiagnosticEventV1DTO[]; droppedCount: number }>;
  clear(): Promise<void>;
}

class IndexedDbDiagnosticsStore implements DiagnosticsStoreLike {
  /** Memoized so `openDb()` (below) opens the connection at most once for
   * this store's whole lifetime — see the module doc's "review round 1"
   * note on the previous per-operation-connection defect. */
  private dbPromise: Promise<IDBDatabase> | null = null;

  constructor(
    private readonly openDbOnce: () => Promise<IDBDatabase>,
    private readonly keyRange: IDBKeyRangeCtor,
  ) {}

  private openDb(): Promise<IDBDatabase> {
    if (this.dbPromise === null) {
      this.dbPromise = this.openDbOnce();
    }
    return this.dbPromise;
  }

  async append(events: DiagnosticEventV1DTO[], nowMs: number): Promise<number> {
    if (events.length === 0) {
      return 0;
    }
    try {
      const db = await this.openDb();
      const tx = db.transaction([EVENTS_STORE, META_STORE], "readwrite");
      const eventsStore = tx.objectStore(EVENTS_STORE);
      const counters = await readCounters(tx);

      for (const event of events) {
        eventsStore.add(event);
        counters.totalBytes += byteLengthOf(event);
      }

      const cutoffMs = nowMs - DIAGNOSTICS_RETENTION_MS;
      const aged = await evictExpired(tx, cutoffMs, { keyRange: this.keyRange });
      counters.totalBytes -= aged.freedBytes;

      const overBudget = await evictOverBudget(tx, counters.totalBytes, DIAGNOSTICS_MAX_BYTES);
      counters.totalBytes -= overBudget.freedBytes;

      const droppedThisAppend = aged.droppedCount + overBudget.droppedCount;
      counters.droppedCount += droppedThisAppend;
      tx.objectStore(META_STORE).put(counters);

      await promisifyTransaction(tx);
      return droppedThisAppend;
    } catch {
      // A quota error, a blocked upgrade, an aborted transaction — none of
      // it may reach the caller. Capture, sync and startup all keep
      // running whether or not this write actually landed.
      return 0;
    }
  }

  async exportAll(): Promise<{ events: DiagnosticEventV1DTO[]; droppedCount: number }> {
    try {
      const db = await this.openDb();
      const tx = db.transaction([EVENTS_STORE, META_STORE], "readonly");
      const events: DiagnosticEventV1DTO[] = [];
      const store = tx.objectStore(EVENTS_STORE);
      await new Promise<void>((resolve, reject) => {
        const cursorRequest = store.openCursor();
        cursorRequest.onsuccess = () => {
          const cursor = cursorRequest.result;
          if (cursor === null) {
            resolve();
            return;
          }
          events.push(cursor.value as DiagnosticEventV1DTO);
          cursor.continue();
        };
        cursorRequest.onerror = () => reject(cursorRequest.error);
      });
      const counters = await readCounters(tx);
      await promisifyTransaction(tx);
      return { events, droppedCount: counters.droppedCount };
    } catch {
      return { events: [], droppedCount: 0 };
    }
  }

  async clear(): Promise<void> {
    try {
      const db = await this.openDb();
      const tx = db.transaction([EVENTS_STORE, META_STORE], "readwrite");
      tx.objectStore(EVENTS_STORE).clear();
      tx.objectStore(META_STORE).clear();
      await promisifyTransaction(tx);
    } catch {
      // Swallowed — see the module doc's "never throws" contract. A clear
      // that silently did not happen is surfaced only by a later export
      // still showing events, never by an exception into the dispatcher.
    }
  }
}

/** Builds the store `diagnostics-journal.ts` uses. `factory` is threaded
 * through to `openDiagnosticsDb` for the same test-injection reason;
 * `keyRange` is the `IDBKeyRange` constructor the age-eviction sweep needs
 * — defaulted to the ambient global (present in every real browser and
 * `SharedWorker` global scope), and separately injectable because Node's
 * test environment has no such global of its own (`fake-indexeddb` exports
 * one for `diagnostics-store.test.ts` to pass here). */
export function createDiagnosticsStore(
  factory?: IDBFactory,
  keyRange: IDBKeyRangeCtor = IDBKeyRange,
): DiagnosticsStoreLike {
  return new IndexedDbDiagnosticsStore(() => openDiagnosticsDb(factory), keyRange);
}
