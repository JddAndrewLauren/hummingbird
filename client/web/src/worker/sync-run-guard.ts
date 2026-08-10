// Issue #184's in-flight guard for the shared ADR-0007 cadence.
//
// The cadence controller (`shell/sync-cadence.ts`) fires `run` once per
// triggering event with no notion of whether a previous `run` is still
// going — verified during triage as a real, not theoretical, defect: the
// task binding's serial queue (`serial-queue.ts`) *abandons* a request after
// `TASK_REQUEST_TIMEOUT_MS`, which resolves the queue's own wait and moves
// on, but leaves the abandoned handler's promise floating un-awaited (there
// is no `AbortSignal` across the wasm boundary to actually cancel it). A
// cycle slower than the 60-second foreground timer, or than the 30-second
// abandon bound against any other trigger, can therefore overlap the next
// `runSync` — which finds the core already checked out and resolves
// `"busy"`, the outcome the protocol's own doc calls "never-in-practice".
//
// This module wraps `run` rather than the queue itself: teaching the
// generic serial queue about `runSync` specifically would put sync-cycle
// knowledge inside a queue every binding shares, and the queue's own
// abandon-on-timeout path is a deliberate, unrelated fix (#95) that must
// keep working exactly as it does today (2026-08-09 triage ruling).
//
// This is a sibling of `shell/sync-cadence.ts`, not a change to it: the
// cadence's own trigger/pause logic (`onOpen`/`onFocus`/etc., the timer
// gate) stays untouched, and `core.worker.ts` wraps the `run` callback it
// passes to `createSyncCadence` with `createSyncRunGuard` instead. Kept
// free of `SharedWorkerGlobalScope`/wasm entirely, same as the cadence
// itself, so a vitest (node) test can exercise it directly with a fake
// `runSync` — no DOM, no wasm.

/** Configuration for [`createSyncRunGuard`]. */
export interface SyncRunGuardOptions {
  /** The guard's own release bound: how long an in-flight run may hold the
   * slot before the guard gives up waiting on it and treats the slot as
   * free again, even though the run's promise has not settled. Chosen to
   * mirror `TASK_REQUEST_TIMEOUT_MS` (`task-worker.ts`) — the same bound
   * the underlying serial queue already uses to abandon a hung request —
   * rather than inventing a second, independent number for the same
   * "how long is too long" question. The caller (`core.worker.ts`) passes
   * that constant in explicitly; this module does not import it, so it has
   * no compile-time dependency on the task binding.
   *
   * This bound is what keeps the guard from wedging the cadence forever: a
   * `runSync` whose promise never settles (a genuinely hung request, not
   * just a slow one — the underlying queue itself already gives up on those
   * at the same bound) would otherwise hold `inFlight` true permanently,
   * silently swallowing every future trigger into an ever-overwritten
   * pending slot that never runs. Once this bound elapses the guard
   * releases the slot and, if a trigger arrived meanwhile, starts it
   * immediately — the straggling run's eventual settlement (if it ever
   * comes) is a no-op against the guard, guarded by generation below, so it
   * cannot corrupt whatever run has taken its place. */
  releaseMs: number;
}

/** Wraps an async `runSync` (a `SyncCadenceTrigger => Promise<void>`, e.g.
 * `core.worker.ts`'s `taskEnqueueReady.then((enqueue) => enqueue({...}))`)
 * with issue #184's in-flight guard, and returns a fire-and-forget function
 * of the same shape `createSyncCadence`'s `run` parameter expects.
 *
 * At most one call to `runSync` is in flight at a time. A trigger arriving
 * while one is in flight does not start a second one — it becomes (or
 * replaces) the single pending follow-up, so any number of triggers arriving
 * during one in-flight run still produce exactly one follow-up run, which
 * starts the instant the in-flight one resolves (or is released — see
 * `SyncRunGuardOptions.releaseMs`). A trigger arriving once no run is in
 * flight starts immediately, synchronously, with no added latency. */
export function createSyncRunGuard<Trigger>(
  runSync: (trigger: Trigger) => Promise<void>,
  options: SyncRunGuardOptions,
): (trigger: Trigger) => void {
  let inFlight = false;
  let pending: Trigger | null = null;
  // Guards against a straggling run's late `.finally` (fired after the
  // release bound already freed the slot for a new run) mistakenly
  // releasing that NEW run's slot too — see the module doc's account of
  // why `releaseMs` alone is not enough.
  let generation = 0;

  function start(trigger: Trigger): void {
    inFlight = true;
    const thisGeneration = ++generation;

    const releaseTimer = setTimeout(() => {
      settle(thisGeneration);
    }, options.releaseMs);

    void runSync(trigger)
      .catch(() => {
        // A rejected `runSync` still frees the slot — the caller
        // (`core.worker.ts`'s `runSync`) is expected to report its own
        // failures; this guard's only job is concurrency, not error
        // reporting.
      })
      .finally(() => {
        clearTimeout(releaseTimer);
        settle(thisGeneration);
      });
  }

  function settle(thisGeneration: number): void {
    // Stale: either a later run already replaced this one (a new
    // generation started after this one's release bound fired), or this
    // generation already settled once (the release timer and the promise's
    // own `.finally` both call `settle` and only the first should count).
    if (thisGeneration !== generation || !inFlight) {
      return;
    }
    inFlight = false;
    if (pending !== null) {
      const next = pending;
      pending = null;
      start(next);
    }
  }

  return (trigger: Trigger) => {
    if (inFlight) {
      pending = trigger;
      return;
    }
    start(trigger);
  };
}
