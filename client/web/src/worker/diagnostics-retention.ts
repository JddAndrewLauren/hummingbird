// #707's retention policy for the SharedWorker's diagnostic journal, as pure
// logic over a plain description of what is stored — no IndexedDB import
// here, so it is unit-testable in isolation the same way `dispatch.ts` and
// `request-router.ts` keep their own decision logic free of the wasm/IDB
// glue that would otherwise make it untestable (see those files' own
// module docs for the same argument). `diagnostics-store.ts` is the only
// caller, and owns turning this plan into real `IDBObjectStore` deletes.
//
// Two bounds apply, both from #707's Agent Brief: 72 hours (by an
// **injected** clock — `nowMs` is always a caller-supplied argument, never
// sampled here) and 10 MiB (by cumulative serialized byte size). Age
// eviction runs first, then size eviction over whatever survives it — a
// record can be dropped by either rule, never both counted twice for the
// same record, since `evictKeys` is a `Set`-backed dedupe.
//
// Eviction always removes the OLDEST surviving records first. "Oldest" is
// primary-key order, not `wallClockMs` order: `diagnostics-store.ts`'s
// object store is an `autoIncrement` IndexedDB store, so key order IS
// insertion order, which is the true write order across a restart (an
// `autoIncrement` counter persists with the database) even if two events
// share a millisecond-resolution timestamp. Records are only ever evicted
// whole — this module never truncates or partially drops one, which is
// what "the oldest *complete* events go first, never a half-written one"
// means in practice: a record only ever exists in the store once fully
// written, so any record this plan can see is by definition complete.

export interface RetentionRecord {
  /** The IndexedDB primary key — `autoIncrement`, so ascending key order is
   * insertion order. */
  key: number;
  wallClockMs: number;
  /** The record's own serialized byte length, computed once at write time
   * (`diagnostics-store.ts`) rather than re-measured here. */
  byteLength: number;
}

export interface RetentionOptions {
  retentionMs: number;
  maxBytes: number;
}

export interface RetentionPlan {
  /** Primary keys to delete, oldest first. */
  evictKeys: number[];
  /** How many records this plan drops — always `evictKeys.length`, kept as
   * its own field so a caller reads intent rather than re-deriving it. */
  droppedCount: number;
}

/** Decides which of `records` (assumed already in ascending key/insertion
 * order — `diagnostics-store.ts`'s cursor reads guarantee this) must be
 * evicted to satisfy both bounds. Never mutates `records`. */
export function planRetention(
  records: readonly RetentionRecord[],
  nowMs: number,
  options: RetentionOptions,
): RetentionPlan {
  const evictKeys: number[] = [];
  const cutoffMs = nowMs - options.retentionMs;

  const survivingAge: RetentionRecord[] = [];
  for (const record of records) {
    if (record.wallClockMs < cutoffMs) {
      evictKeys.push(record.key);
    } else {
      survivingAge.push(record);
    }
  }

  let totalBytes = survivingAge.reduce((sum, record) => sum + record.byteLength, 0);
  let index = 0;
  while (totalBytes > options.maxBytes && index < survivingAge.length) {
    const record = survivingAge[index];
    evictKeys.push(record.key);
    totalBytes -= record.byteLength;
    index += 1;
  }

  return { evictKeys, droppedCount: evictKeys.length };
}
