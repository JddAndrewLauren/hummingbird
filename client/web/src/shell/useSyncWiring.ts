import { useEffect, useRef, useState } from "react";
import { downloadMirrorSnapshot } from "./mirror-download";
import { createSyncCadence, SYNC_TIMER_MS } from "./sync-cadence";
import type { CoreStatus, TaskState } from "../store/store";
import {
  requestDeadLetters,
  requestMirrorSnapshot,
  requestQueueDepth,
  runTaskSync,
  type WorkerLike,
} from "../store/worker-client";

// S9's ADR-0007 cadence, wired: on open, on reconnect, on window focus, and
// the 60-second foreground timer — see `sync-cadence.ts` for the pure
// decision logic and why "at the tail of every queue drain" needs no wiring
// here (it is built into `Core::run` itself). Thin wire-up over that tested
// module, the same shape `useCalendarWiring.ts` and `useTaskTokenWiring.ts`
// already use.
//
// Gated on `hasToken`: a device with no task credential yet has nothing for
// a cycle to do (`Core::run` reports `no_credential` without ever reaching
// the network), so there is no reason to run a timer or attach listeners
// before one exists.

export interface SyncWiring {
  /** Re-sampled every 30s (independent of `useCalendarWiring.ts`'s own
   * clock, which freezes on a device with no calendar opt-in) so the sync
   * status's "as of" label stays live for every device, not just ones with
   * a connected calendar. */
  nowMs: number;
  /** Requests the current mirror and, once it answers, writes it to disk.
   * S9's mirror download button. */
  handleDownloadMirror: () => void;
}

/** How often the sync-status "as of" label re-samples the clock — same
 * granularity as `useCalendarWiring.ts`'s own `CLOCK_TICK_MS`, and for the
 * same reason: `sync-status.ts`'s coarsest unit is a minute, so anything
 * finer is wasted renders. */
const STATUS_CLOCK_TICK_MS = 30 * 1000;

export function useSyncWiring(
  worker: WorkerLike,
  status: CoreStatus,
  hasToken: boolean,
  task: Pick<TaskState, "lastSyncOutcome" | "mirrorSnapshot">,
): SyncWiring {
  // A plain function declaration, not an inline closure passed to a hook —
  // same shape `useCalendarWiring.ts`'s own handlers use, and what keeps the
  // per-cycle clock/jitter sampling (below) from reading as render-time
  // impurity: it only ever runs later, from an effect or an event listener.
  function runCycle(trigger: "user" | "timer") {
    // `forceFullSweep: false` — the normal pull is the delta since the
    // mirror's own version (ADR-0008 amendment); a full sweep is the
    // correctness backstop `adapter`'s own module docs describe as running
    // "on app open and daily", which is a separate, unbuilt cadence this
    // hook does not yet own (see the PR/issue notes). `Math.random()` is
    // the caller-injected jitter `Core::run` requires on bare wasm32 (no
    // panic-free RNG of its own).
    runTaskSync(worker, Date.now(), trigger, false, Math.random());
  }

  // A ref, not a `useMemo`: the cadence closure must stay stable across
  // renders (it is what every effect below attaches as a listener).
  const cadenceRef = useRef<ReturnType<typeof createSyncCadence> | null>(null);
  cadenceRef.current ??= createSyncCadence(runCycle);
  const cadence = cadenceRef.current;

  const ready = status === "ready" && hasToken;
  const [nowMs, setNowMs] = useState(() => Date.now());

  // The status clock — see `SyncWiring.nowMs`'s own doc for why this is
  // independent of the calendar wiring's clock.
  useEffect(() => {
    if (!ready) {
      return;
    }
    const id = window.setInterval(() => setNowMs(Date.now()), STATUS_CLOCK_TICK_MS);
    return () => window.clearInterval(id);
  }, [ready]);

  // On open / core start (ADR-0007).
  useEffect(() => {
    if (!ready) {
      return;
    }
    cadence.onOpen();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready]);

  // On reconnect — the browser's own connectivity signal (ADR-0007), not
  // the Google Calendar credential's reconnect flow.
  useEffect(() => {
    if (!ready) {
      return;
    }
    function handleOnline() {
      cadence.onReconnect();
    }
    window.addEventListener("online", handleOnline);
    return () => window.removeEventListener("online", handleOnline);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready]);

  // On window focus (ADR-0007). Exactly one listener for the document's
  // lifetime while ready — `sync-cadence.ts`'s own doc is what keeps one
  // focus event mapping to exactly one cycle.
  useEffect(() => {
    if (!ready) {
      return;
    }
    function handleFocus() {
      cadence.onFocus();
    }
    window.addEventListener("focus", handleFocus);
    return () => window.removeEventListener("focus", handleFocus);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready]);

  // The 60-second foreground timer (ADR-0007), paused while hidden or
  // offline per `shouldRunTimerTick` — proven in `sync-cadence.test.ts`.
  useEffect(() => {
    if (!ready) {
      return;
    }
    const id = window.setInterval(() => {
      cadence.onTimerTick(document.hidden, navigator.onLine);
    }, SYNC_TIMER_MS);
    return () => window.clearInterval(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready]);

  // Refreshes the sync-status reads once ready, and again after every cycle
  // (the "drain tail" is exactly when the queue depth and journal can have
  // changed) — cheap, and keeps the status indicator honest without a
  // separate poll.
  useEffect(() => {
    if (!ready) {
      return;
    }
    requestQueueDepth(worker);
    requestDeadLetters(worker);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready, task.lastSyncOutcome]);

  // The download itself is a one-off action, not durable UI state: a click
  // requests a fresh snapshot and this effect writes it to disk the moment
  // the broadcast lands, rather than routing the click through a
  // request/response pairing this protocol doesn't have (every
  // worker->view message is a broadcast, not a reply to one sender —
  // protocol.ts). `pendingRef` distinguishes "a download is owed" from any
  // other view's snapshot arriving first.
  const pendingDownloadRef = useRef(false);
  useEffect(() => {
    if (!pendingDownloadRef.current || task.mirrorSnapshot === null) {
      return;
    }
    pendingDownloadRef.current = false;
    downloadMirrorSnapshot(task.mirrorSnapshot, Date.now());
  }, [task.mirrorSnapshot]);

  function handleDownloadMirror() {
    pendingDownloadRef.current = true;
    requestMirrorSnapshot(worker);
  }

  return { nowMs, handleDownloadMirror };
}
