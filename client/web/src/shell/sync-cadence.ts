// ADR-0007's sync cadence, as pure decision logic — kept free of `window`,
// `document` and the worker itself so it is unit-testable without a DOM
// environment (this repo's vitest config is `environment: "node"`; see
// `useSyncWiring.ts` for the thin hook that wires DOM events into this).
//
// ADR-0007's own cadence section: "Sweep on app open / core start, on
// reconnect, on window focus, and at the tail of every queue drain (built
// into the cycle). Foreground timer: 60 seconds, paused entirely while
// hidden or backgrounded." The queue-drain trigger is already built into
// `Core::run` itself (drain, then pull, every call) — nothing here re-fires
// a second cycle for it. What this module owns is the other four: open,
// reconnect, focus, and the foreground timer.

/** ADR-0007: "Foreground timer: 60 seconds, paused entirely while hidden or
 * backgrounded." Distinct from — and far more frequent than —
 * `useCalendarWiring.ts`'s 15-minute `TIMER_INTERVAL_MS` (ADR-0005): the two
 * cite different ADRs because they are different cadences for different
 * data, not one constant renamed twice. */
export const SYNC_TIMER_MS = 60_000;

/** Whether the foreground timer's tick should actually fire a cycle —
 * ADR-0007: "paused entirely while hidden or backgrounded." A device with
 * no network is not "backgrounded", but running a cycle against it would
 * only manufacture a failed cycle to report; gating on both keeps a
 * predictably-offline tick silent rather than noisy. */
export function shouldRunTimerTick(hidden: boolean, online: boolean): boolean {
  return !hidden && online;
}

/** What triggered one sync cycle — carried through only for tests and
 * logging; the `runSync` request `core.worker.ts`'s cadence sink actually
 * posts to the task queue only distinguishes `"user"` from `"timer"`
 * (`Core::run`'s own `Trigger`), which `toCoreTrigger` below maps this
 * onto. */
export type SyncCadenceTrigger = "open" | "reconnect" | "focus" | "manual" | "timer";

/** `Core::run`'s own `Trigger` only has two spellings, and the mapping is
 * NOT "every ADR-0007 cadence trigger except the timer is user-facing".
 * ADR-0007 lists "on window focus" among its four cadence triggers in one
 * sentence, and states "backoff is reset by any user-facing trigger" in a
 * separate one — and per issue #190's triage ruling, a window-focus event
 * was never the gesture that second sentence is about. A focus event says a
 * window came forward; it is an ambient signal, not a request, and the ADR
 * reserves gesture language for manual refresh ("Manual refresh is the same
 * cycle, user-invoked"). So `"focus"` joins `"timer"` on the right-hand
 * side here, alongside the unattended foreground timer — both leave backoff
 * alone. Only `"open"`, `"reconnect"` and `"manual"` are genuine
 * user-facing requests and map to `"user"`, which is what makes issue #194's
 * "a manual refresh resets backoff" true for free: it costs nothing beyond
 * sending the right trigger spelling, because `Core::run` already resets
 * backoff on `"user"` (`client/core/src/sync/cycle.rs`).
 *
 * Consequence, all intended (issue #190): outside backoff a focus behaves
 * exactly as before (a prompt cycle); during backoff a focus produces a
 * cycle the core declines as "not ready yet" instead of collapsing the
 * backoff window, so alt-tabbing at any rate can no longer extend an
 * outage's request rate beyond the backoff schedule. This does not amend
 * ADR-0007 — it is an interpretation of it, recorded here rather than in the
 * ADR itself. */
export function toCoreTrigger(trigger: SyncCadenceTrigger): "user" | "timer" {
  return trigger === "timer" || trigger === "focus" ? "timer" : "user";
}

/** Precedence for issue #184's guard: when a trigger arrives while a run is
 * already in flight and one is already waiting in the guard's single
 * `pending` slot, this decides which of the two survives — never a bare
 * "last one wins", because trigger identity is load-bearing past the guard:
 * `core.worker.ts` reads it to decide `forceFullSweep` (ADR-0008/#193: true
 * only on `"open"`) and `toCoreTrigger` reads it to decide whether the
 * eventual `runSync` resets backoff (`"user"` vs `"timer"`, ADR-0007/#194).
 * A later, lower-priority trigger silently overwriting a higher-priority one
 * already waiting would lose real behavior, not just delay it: an
 * `"open"` overwritten by a later `"focus"`/`"timer"` drops ADR-0008's
 * app-open full-sweep backstop for the rest of the `SharedWorker`'s
 * lifetime (`dispatch.ts`'s `openTrigger` only ever fires it once), and a
 * `"reconnect"`/`"manual"` overwritten by a later `"focus"` or `"timer"`
 * silently demotes a user-facing backoff reset to an unattended one — the
 * exact outage-recovery path this precedence exists to protect: network
 * returns while a run is in flight, `"reconnect"` waits in the pending
 * slot, the user alt-tabs, and the follow-up cycle must still reset
 * backoff.
 *
 * Precedence: `"open"` (ADR-0008's full-sweep backstop) beats the two
 * user-facing triggers (`"reconnect"`, `"manual"` — the ones
 * `toCoreTrigger` maps to `"user"`, resetting backoff), which beat
 * `"focus"` (issue #190: an ambient signal that maps to `"timer"` and
 * leaves backoff alone, but still kept above the unattended timer so the
 * merged trigger's identity survives for logging/tests), which beats
 * `"timer"`. Between the two user-facing triggers there is no behavioral
 * difference downstream of the guard — both map to `"user"` and neither
 * forces a full sweep — so a tie keeps whichever is `incoming`, the same
 * "most recent wins" rule the guard used before this fix, scoped down to
 * only the cases where it cannot lose information. */
export function mergePendingSyncTrigger(
  pending: SyncCadenceTrigger,
  incoming: SyncCadenceTrigger,
): SyncCadenceTrigger {
  return syncTriggerPriority(incoming) >= syncTriggerPriority(pending) ? incoming : pending;
}

function syncTriggerPriority(trigger: SyncCadenceTrigger): number {
  if (trigger === "open") return 3;
  if (trigger === "focus") return 1; // below "reconnect"/"manual": never demotes a backoff reset
  if (trigger === "timer") return 0;
  return 2; // "reconnect" | "manual" — the backoff-resetting pair
}

export interface SyncCadence {
  /** App open / core start (ADR-0007). Call once the core is ready and a
   * task credential is known. */
  onOpen: () => void;
  /** Network reconnect (ADR-0007's "on reconnect") — the browser's `online`
   * event, not the Google Calendar credential's own reconnect flow (a
   * different subsystem entirely, `useCalendarWiring.ts`'s). */
  onReconnect: () => void;
  /** Window focus (ADR-0007). Fires exactly one cycle per call — this
   * function does not itself debounce repeated calls, so a caller must wire
   * exactly one DOM listener per document, not one per component. */
  onFocus: () => void;
  /** Manual refresh (ADR-0007: "the same cycle, user-invoked; no special
   * path", issue #194). Routed through the shared cadence rather than posted
   * straight to the task queue, so a manual press stays the same one cycle
   * every other trigger uses, and any future in-flight coalescing (#184)
   * covers it too. */
  onManual: () => void;
  /** One foreground-timer tick. No-ops per [`shouldRunTimerTick`] rather
   * than firing and letting the cycle itself discover there is nothing
   * useful to do — a paused tick must cost nothing, not even a message to
   * the worker. */
  onTimerTick: (hidden: boolean, online: boolean) => void;
}

/** Builds the cadence controller: `run` is called with the real
 * [`SyncCadenceTrigger`] — not `toCoreTrigger`'s collapsed `"user" | "timer"`
 * — exactly once per triggering event this controller is told about. The
 * one caller (`core.worker.ts`) needs the un-collapsed spelling to decide
 * `forceFullSweep` (ADR-0008: true on `"open"` only, #193) and calls
 * `toCoreTrigger` itself when it builds the actual sync request; that
 * mapping rule stays defined here so it has exactly one owner. Kept free of
 * `setInterval`/DOM listeners entirely — see the module doc. */
export function createSyncCadence(run: (trigger: SyncCadenceTrigger) => void): SyncCadence {
  function fire(trigger: SyncCadenceTrigger) {
    run(trigger);
  }
  return {
    onOpen: () => fire("open"),
    onReconnect: () => fire("reconnect"),
    onFocus: () => fire("focus"),
    onManual: () => fire("manual"),
    onTimerTick: (hidden, online) => {
      if (shouldRunTimerTick(hidden, online)) {
        fire("timer");
      }
    },
  };
}
