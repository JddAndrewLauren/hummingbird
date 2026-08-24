// #707's retention policy for the SharedWorker's diagnostic journal, as pure
// logic over a plain description of what is stored — no IndexedDB import
// here, so it is unit-testable in isolation the same way `dispatch.ts` and
// `request-router.ts` keep their own decision logic free of the wasm/IDB
// glue that would otherwise make it untestable (see those files' own
// module docs for the same argument).
//
// **This module is a tested SPEC, not called at runtime by
// `diagnostics-store.ts`.** It was, until review round 1 of PR #736 caught
// a real cost: calling this against the WHOLE store's records on every
// single `append` meant reading and re-`JSON.stringify`-ing up to 10 MiB on
// every append — every 250ms during exactly the stall this journal exists
// to observe. `diagnostics-store.ts` now realises the identical policy
// (age before size, oldest-first, never double-counted) directly over
// indexed cursors — an indexed range query for the age sweep, a
// primary-key cursor that stops the instant the running byte total drops
// under budget for the size sweep — touching only the records it actually
// evicts. This file's own tests (`diagnostics-retention.test.ts`) are what
// pin the POLICY correct against a plain list, independent of that IO
// concern; `diagnostics-store.test.ts` is what proves the store's
// cursor-based realisation produces the same observable outcomes.
//
// Two bounds apply, both from #707's Agent Brief: 72 hours (by an
// **injected** clock — `nowMs` is always a caller-supplied argument, never
// sampled here) and 10 MiB (by cumulative serialized byte size). Age
// eviction runs first, then size eviction over whatever survives it — a
// record can be dropped by either rule, never both counted twice for the
// same record: `survivingAge` and the aged-out records below are two
// disjoint partitions of the same input list (every record lands in
// exactly one), so nothing pushed into `evictKeys` by the age pass is ever
// a candidate for the size pass, with no `Set` or other de-duplication
// needed to keep that true.
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
  /** The record's own serialized byte length. */
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
 * order) must be evicted to satisfy both bounds. Never mutates `records`.
 * The reference implementation of the policy `diagnostics-store.ts`
 * realises over indexed cursors instead — see the module doc above for why
 * the two are not the same code path. */
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
