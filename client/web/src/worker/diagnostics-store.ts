import type { DiagnosticEventV1DTO } from "../store/protocol";
import { planRetention, type RetentionRecord } from "./diagnostics-retention";

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
// A second `wall_clock_ms` index exists only so retention's age sweep does
// not have to scan every record to find the old ones.
//
// Retention policy itself (`planRetention`) is pure logic in
// `diagnostics-retention.ts`; this module's only job is turning a plan into
// real deletes and keeping the `meta` store's running totals (cumulative
// byte size and dropped-event count) in step with them.
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
 * `core.worker.ts` calls this with (a `SharedWorkerGlobalScope` has one). */
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
  constructor(private readonly openDb: () => Promise<IDBDatabase>) {}

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
        const byteLength = byteLengthOf(event);
        eventsStore.add(event);
        counters.totalBytes += byteLength;
      }

      const allRecords = await this.readAllRecords(tx);
      const plan = planRetention(allRecords, nowMs, {
        retentionMs: DIAGNOSTICS_RETENTION_MS,
        maxBytes: DIAGNOSTICS_MAX_BYTES,
      });
      const byKey = new Map(allRecords.map((record) => [record.key, record]));
      for (const key of plan.evictKeys) {
        eventsStore.delete(key);
        const record = byKey.get(key);
        if (record !== undefined) {
          counters.totalBytes -= record.byteLength;
        }
      }
      counters.droppedCount += plan.droppedCount;
      tx.objectStore(META_STORE).put(counters);

      await promisifyTransaction(tx);
      return plan.droppedCount;
    } catch {
      // A quota error, a blocked upgrade, an aborted transaction — none of
      // it may reach the caller. Capture, sync and startup all keep
      // running whether or not this write actually landed.
      return 0;
    }
  }

  private async readAllRecords(tx: IDBTransaction): Promise<(RetentionRecord & { event: DiagnosticEventV1DTO })[]> {
    const records: (RetentionRecord & { event: DiagnosticEventV1DTO })[] = [];
    const store = tx.objectStore(EVENTS_STORE);
    await new Promise<void>((resolve, reject) => {
      const cursorRequest = store.openCursor();
      cursorRequest.onsuccess = () => {
        const cursor = cursorRequest.result;
        if (cursor === null) {
          resolve();
          return;
        }
        const event = cursor.value as DiagnosticEventV1DTO;
        records.push({
          key: cursor.key as number,
          wallClockMs: event.wall_clock_ms,
          byteLength: byteLengthOf(event),
          event,
        });
        cursor.continue();
      };
      cursorRequest.onerror = () => reject(cursorRequest.error);
    });
    return records;
  }

  async exportAll(): Promise<{ events: DiagnosticEventV1DTO[]; droppedCount: number }> {
    try {
      const db = await this.openDb();
      const tx = db.transaction([EVENTS_STORE, META_STORE], "readonly");
      const records = await this.readAllRecords(tx);
      const counters = await readCounters(tx);
      await promisifyTransaction(tx);
      return { events: records.map((record) => record.event), droppedCount: counters.droppedCount };
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
 * through to `openDiagnosticsDb` for the same test-injection reason. */
export function createDiagnosticsStore(factory?: IDBFactory): DiagnosticsStoreLike {
  return new IndexedDbDiagnosticsStore(() => openDiagnosticsDb(factory));
}
