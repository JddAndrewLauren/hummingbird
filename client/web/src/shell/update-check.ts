// The decidable half of "check for a new version hourly and on window
// focus" — extracted so it is provable without a browser, `refresh-gate.ts`
// + its test being the template. Every effect (the check itself, the clock)
// is injected.

/** The background cadence. Nothing here reads a clock to schedule it —
 * `useAppUpdate.ts` owns the `window.setInterval`. */
export const UPDATE_CHECK_INTERVAL_MS = 60 * 60 * 1000;

/** The floor between two actual checks, whatever asked for them. A focus
 * event says a window came forward — an ambient signal, not a gesture — so
 * alt-tabbing at any rate must not turn into a request rate against the
 * origin. The same interpretation `toCoreTrigger` records for #190, applied
 * to a different ambient signal. */
export const MIN_CHECK_GAP_MS = 5 * 60 * 1000;

export interface UpdateChecker {
  /** The hourly tick and the focus signal both land here. */
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
