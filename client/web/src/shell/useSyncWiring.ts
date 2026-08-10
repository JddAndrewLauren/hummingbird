import { useEffect, useRef, useState } from "react";
import { downloadMirrorSnapshot } from "./mirror-download";
import type { CoreStatus } from "../store/store";
import {
  reportViewVisibility,
  requestDeadLetters,
  requestMirrorSnapshot,
  requestQueueDepth,
  setMirrorSnapshotHandler,
  triggerSyncFocus,
  triggerSyncManual,
  type WorkerLike,
} from "../store/worker-client";

// S9's shell wiring for the sync-status affordance. As of round-1 review of
// PR #181, this file does NOT own ADR-0007's cadence — a per-view
// `setInterval` used to multiply cycles with open-tab count, which
// contradicts ADR-0010's "a second tab is a view, not a second cycle".
// `core.worker.ts` now owns the shared timer and the open/reconnect
// triggers itself; this hook's only remaining cadence-related job is
// reporting the two facts the worker's global scope cannot observe on its
// own — this view's page visibility, and this view's own focus events —
// and requesting the read-only sync-status data (queue depth, dead
// letters, the mirror) that Settings renders.

export interface SyncWiring {
  /** Re-sampled every 30s so the sync status's "as of" label stays live —
   * independent of `useCalendarWiring.ts`'s own clock, which freezes on a
   * device with no calendar opt-in. */
  nowMs: number;
  /** Requests the current mirror and, once it answers, writes it to disk.
   * S9's mirror download button. */
  handleDownloadMirror: () => void;
  /** Issue #194: the header refresh control's task leg — fires the ADR-0007
   * cycle through the shared cadence (`triggerSyncManual`), same as every
   * other trigger. `App.tsx` calls this only when a task token is present
   * (`shell/refresh-gate.ts`). */
  handleManualSync: () => void;
}

/** How often the sync-status "as of" label re-samples the clock — same
 * granularity as `useCalendarWiring.ts`'s own `CLOCK_TICK_MS`, and for the
 * same reason: `sync-status.ts`'s coarsest unit is a minute, so anything
 * finer is wasted renders. */
const STATUS_CLOCK_TICK_MS = 30 * 1000;

export function useSyncWiring(worker: WorkerLike, status: CoreStatus): SyncWiring {
  const ready = status === "ready";
  const [nowMs, setNowMs] = useState(() => Date.now());

  // The status clock — see `SyncWiring.nowMs`'s own doc.
  useEffect(() => {
    if (!ready) {
      return;
    }
    const id = window.setInterval(() => setNowMs(Date.now()), STATUS_CLOCK_TICK_MS);
    return () => window.clearInterval(id);
  }, [ready]);

  // Reports this view's own page visibility to the shared cadence
  // (`core.worker.ts`'s `VisibilityTracker`) — sent once on mount (the
  // worker has no other way to learn the current state) and again on every
  // `visibilitychange`.
  useEffect(() => {
    if (!ready) {
      return;
    }
    reportViewVisibility(worker, document.hidden);
    function handleVisibilityChange() {
      reportViewVisibility(worker, document.hidden);
    }
    document.addEventListener("visibilitychange", handleVisibilityChange);
    return () => document.removeEventListener("visibilitychange", handleVisibilityChange);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready]);

  // ADR-0007's "on window focus" trigger — forwarded to the shared cadence
  // rather than run locally; see `protocol.ts`'s `SyncCadenceRequest` doc
  // for why this one is not deduplicated across views the way the timer is.
  // Issue #190: this still fires unconditionally on every focus — the
  // cadence itself is unchanged — but the resulting cycle now carries the
  // core's `"timer"` trigger (`sync-cadence.ts`'s `toCoreTrigger`), so a
  // focus never resets ADR-0007's backoff the way open/reconnect/manual do.
  useEffect(() => {
    if (!ready) {
      return;
    }
    function handleFocus() {
      triggerSyncFocus(worker);
    }
    window.addEventListener("focus", handleFocus);
    return () => window.removeEventListener("focus", handleFocus);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready]);

  // Refreshes the sync-status reads once, on becoming ready — this is what
  // lets a view connecting mid-session (or reconnecting) render a populated
  // badge and journal without waiting for the next cycle, per issue #191's
  // brief. It used to also re-fire after every completed cycle, keyed on
  // `TaskState.syncOutcomeSeq` (round-2 review of PR #181: keying on the
  // outcome's own `kind` froze the refresh, since steady state is
  // `"completed"` forever and a dead letter arrives INSIDE a completed
  // outcome rather than changing its `kind`). Issue #191 moved that
  // per-cycle refresh into the worker instead: `worker/task-worker.ts`'s
  // `runSync` branch now pushes `queueDepth`/`deadLetters` unsolicited at
  // the tail of every cycle, broadcast to every connected view the same as
  // `syncOutcome` already was, so N views cost one wasm read per cycle
  // instead of N. See protocol.ts's `queueDepth`/`deadLetters` docs and
  // `TaskState.syncOutcomeSeq`'s own doc for why the counter itself stays.
  useEffect(() => {
    if (!ready) {
      return;
    }
    requestQueueDepth(worker);
    requestDeadLetters(worker);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready]);

  // The download itself is a one-off action, not durable UI state: a click
  // requests a fresh snapshot and this registration writes it to disk the
  // moment the broadcast lands, rather than routing the value through the
  // store — see `worker-client.ts`'s `mirrorSnapshotHandler` doc for why a
  // shared broadcast with no directed reply must not be retained there.
  const pendingDownloadRef = useRef(false);
  useEffect(() => {
    setMirrorSnapshotHandler((mirror) => {
      if (pendingDownloadRef.current) {
        pendingDownloadRef.current = false;
        downloadMirrorSnapshot(mirror, Date.now());
      }
    });
    return () => setMirrorSnapshotHandler(null);
  }, []);

  function handleDownloadMirror() {
    pendingDownloadRef.current = true;
    requestMirrorSnapshot(worker);
  }

  // Issue #194: the manual refresh's task leg. `triggerSyncManual` routes
  // through the shared cadence in `core.worker.ts`, not a bespoke `runSync`
  // straight to the task queue — see that function's own doc.
  function handleManualSync() {
    triggerSyncManual(worker);
  }

  return { nowMs, handleDownloadMirror, handleManualSync };
}
