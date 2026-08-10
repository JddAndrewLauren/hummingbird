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
}

/** How often the sync-status "as of" label re-samples the clock — same
 * granularity as `useCalendarWiring.ts`'s own `CLOCK_TICK_MS`, and for the
 * same reason: `sync-status.ts`'s coarsest unit is a minute, so anything
 * finer is wasted renders. */
const STATUS_CLOCK_TICK_MS = 30 * 1000;

export function useSyncWiring(
  worker: WorkerLike,
  status: CoreStatus,
  /** `TaskState.syncOutcomeSeq` — bumps on EVERY completed cycle, which is
   * what makes the per-cycle refresh below actually per-cycle. Round-2
   * review of PR #181: this used to be the outcome's `kind`, which is
   * `"completed"` forever in the steady state, so the refresh effect fired
   * once and froze — a dead letter created later in the session (it arrives
   * inside a *completed* outcome; `deadLettered` is a separate field) never
   * surfaced, and the queue-depth badge stuck at its first reading. */
  syncOutcomeSeq: number,
): SyncWiring {
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

  // Refreshes the sync-status reads once ready, and again after every cycle
  // (the "drain tail" is exactly when the queue depth and journal can have
  // changed) — cheap, and keeps the status indicator honest without a
  // separate poll. Keyed on `syncOutcomeSeq`, a per-cycle counter, precisely
  // BECAUSE the outcome's own fields do not change between steady-state
  // cycles — see this hook's parameter doc.
  useEffect(() => {
    if (!ready) {
      return;
    }
    requestQueueDepth(worker);
    requestDeadLetters(worker);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready, syncOutcomeSeq]);

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

  return { nowMs, handleDownloadMirror };
}
