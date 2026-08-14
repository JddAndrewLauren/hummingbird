// The decidable half of "check for a new version whenever this view comes
// forward, and periodically while it stays there" — extracted so it is
// provable without a browser, `refresh-gate.ts` + its test being the
// template. Every effect (the check itself, the clock) is injected.
//
// `useAppUpdate.ts` owns which signals exist; this module owns only the
// floor between two checks, so adding a sixth signal there needs no change
// here.

/** The background cadence, and the ONLY thing here that is about elapsed
 * time rather than a signal. It now covers just one case — a view left
 * foregrounded and untouched — because every *resume* path has its own
 * signal in `useAppUpdate.ts`; that is why 30 minutes is affordable where
 * an hour was the whole discovery mechanism. Nothing here reads a clock to
 * schedule it: `useAppUpdate.ts` owns the `window.setInterval`. */
export const UPDATE_CHECK_INTERVAL_MS = 30 * 60 * 1000;

/** The floor between two actual checks, whatever asked for them. Every
 * signal feeding this is ambient — a view came forward — rather than a
 * gesture, so alt-tabbing or locking and unlocking a phone at any rate must
 * not turn into a request rate against the origin. The same interpretation
 * `toCoreTrigger` records for #190, applied to a different ambient signal.
 *
 * Kept at 5 minutes deliberately. Flip condition: lower it only if a device
 * is observed missing a deploy *because* its check landed inside this gap —
 * not merely because a device was slow to notice one. */
export const MIN_CHECK_GAP_MS = 5 * 60 * 1000;

export interface UpdateChecker {
  /** Every signal — the periodic tick, and each of the ways a view can come
   * forward — lands here, so the gap below is the single rate limiter. */
  request(): void;
}

export function createUpdateChecker(check: () => void, now: () => number): UpdateChecker {
  // Never checked yet, so the first request always fires — a window that
  // has been open across a deploy should learn about it on the next focus,
  // not an hour later.
  let lastCheckedAtMs: number | null = null;

  return {
    request(): void {
      const at = now();
      if (lastCheckedAtMs !== null && at - lastCheckedAtMs < MIN_CHECK_GAP_MS) {
        // Dropped, never queued: a deferred check would fire at a moment
        // nobody asked for, and a later focus or the hourly tick comes
        // round anyway.
        return;
      }
      lastCheckedAtMs = at;
      check();
    },
  };
}
