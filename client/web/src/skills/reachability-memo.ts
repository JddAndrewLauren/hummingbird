// The reachability memo #274 asks for: "a memo, not a clock." Nothing here
// reads the wall clock or starts anything — every function takes the
// current time as a parameter, so a test drives it with a fixed number and
// the real caller drives it with `Date.now()`. There is no polling loop:
// an entry is written only as the side effect of an actual attempt
// (`route-run.ts`), never by a background sweep.
//
// **Brief, not durable.** `MEMO_TTL_MS` is short on purpose — a home
// machine that just went to sleep must not stay "known dead" long after it
// wakes. The memo is plain data (a `Record`), not a class with a clock
// inside it, so it can live in whatever the caller already owns (a `useRef`
// in `useMicrotaskWiring.ts` here) without becoming a second timer.

export interface ReachabilityMemo {
  [backendId: string]: { reachable: boolean; expiresAtMs: number } | undefined;
}

export const EMPTY_MEMO: ReachabilityMemo = {};

/** How long a memoized answer is trusted before it is treated as unknown
 * again. Deliberately short — this is a memo, not a cache with a real
 * eviction policy. */
export const MEMO_TTL_MS = 30_000;

/** A fresh record of the given backend having just failed to answer. Only
 * "dead" is memoized as a skip-worthy fact: Auto still attempts a backend it
 * has never tried, or one whose memo expired, exactly as if nothing were
 * remembered. */
export function markDead(memo: ReachabilityMemo, backendId: string, nowMs: number, ttlMs = MEMO_TTL_MS): ReachabilityMemo {
  return { ...memo, [backendId]: { reachable: false, expiresAtMs: nowMs + ttlMs } };
}

/** A fresh record of the given backend having just answered. Recorded for
 * symmetry and future use (a picker could show "last answered Xs ago"); no
 * caller currently reads a `reachable: true` entry back out. */
export function markReachable(memo: ReachabilityMemo, backendId: string, nowMs: number, ttlMs = MEMO_TTL_MS): ReachabilityMemo {
  return { ...memo, [backendId]: { reachable: true, expiresAtMs: nowMs + ttlMs } };
}

/** True only while an unexpired memo says this backend just failed to
 * answer. False for "never tried", "expired", and "last answered" alike —
 * the one thing worth skipping ahead of is a *known-recent* dead. */
export function isFreshDead(memo: ReachabilityMemo, backendId: string, nowMs: number): boolean {
  const entry = memo[backendId];
  return entry !== undefined && !entry.reachable && entry.expiresAtMs > nowMs;
}
