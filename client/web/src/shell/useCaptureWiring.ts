import { useEffect } from "react";
import type { CoreStatus } from "../store/store";
import {
  captureTask,
  requestTriageInbox,
  type CaptureFields,
  type WorkerLike,
} from "../store/worker-client";

// Issue #110/S12's capture wiring: requests the triage inbox once the core
// is ready, and again after every sync cycle — the same "refresh once
// ready, then per-cycle" shape `useFrontierWiring.ts` already uses (S10),
// since a completed cycle is exactly when a still-queued capture can have
// been confirmed by the server. `submitCapture` is the one door a screen
// calls through: it mints this capture's seed and hands off to
// `captureTask` (`worker-client.ts`), which is the wire message
// `task-worker.ts` turns into a real `Core::capture` call — durable via
// `SyncCycle::enqueue` before any transport is ever touched (#110's "a
// capture is visible in the list before any network call").
//
// Deliberately thin glue, same as every other `use*Wiring` hook in this
// directory: whether a draft is worth submitting at all is
// `screens/capture-validation.ts`'s pure `canSubmitCapture`, called by the
// screen BEFORE this is ever reached — this hook trusts its caller and
// enqueues whatever it is handed.
export interface CaptureWiring {
  submitCapture: (title: string, nowMs: number, fields?: CaptureFields) => void;
}

/** Mints a fresh, non-deterministic seed for one capture. Only the seed's
 * *uniqueness* matters here — `Core::capture` (client/core) hashes it into
 * the item's deterministic id, and the offline-replay dedup guarantee comes
 * from the same seed being reused only across a retry of the SAME capture,
 * never from this function's output being predictable. */
function mintSeed(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  // A `crypto.randomUUID`-less environment (an old browser, a stripped
  // test double) still needs a seed unique enough that two captures in the
  // same tick never collide — timestamp alone is not, since two captures
  // typed and submitted quickly can land in the same millisecond.
  return `seed-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

/** The wire-level half of a submit: posts the capture, then immediately
 * re-requests the triage inbox right behind it. `task-worker.ts`'s
 * single-file serial queue (issue #105/S7) is what makes the second request
 * land only AFTER `Core::capture` has already returned and the durable
 * enqueue has happened — so the `triageInbox` reply this provokes already
 * carries the fresh optimistic overlay.
 *
 * This is load-bearing, not a nicety: without it, the only thing that ever
 * re-requests the inbox is `useEffect` above, keyed on `syncOutcomeSeq` —
 * which bumps only once a `syncOutcome` broadcast arrives, i.e. AFTER a
 * network cycle. That is the exact inverse of #110's "a capture is visible
 * in the list before any network call" (round-2 review of PR #206).
 *
 * Exported standalone, not inlined in the hook body, so it is testable
 * without rendering a component — the view-side twin of the `worker/*`
 * pure-module split this repo already uses for cadence/routing logic.
 * `seed` defaults to a freshly minted one; a test supplies its own for a
 * deterministic assertion.
 *
 * `fields` (#208) carries the capture box's Energy/Size/Context selections
 * onto the same `captureTask` call — never a follow-up patch — and
 * defaults to `{}` (all three absent), the same resting-state contract
 * `captureTask` itself documents. */
export function submitCaptureRequest(
  worker: WorkerLike,
  title: string,
  nowMs: number,
  fields: CaptureFields = {},
  seed: string = mintSeed(),
): void {
  captureTask(worker, seed, title, "triage", nowMs, fields);
  requestTriageInbox(worker);
}

export function useCaptureWiring(
  worker: WorkerLike,
  status: CoreStatus,
  /** `TaskState.syncOutcomeSeq` — see `useFrontierWiring.ts`'s own
   * parameter doc for why this, not the outcome's `kind`, is what a
   * per-cycle refresh must key on. */
  syncOutcomeSeq: number,
): CaptureWiring {
  const ready = status === "ready";

  useEffect(() => {
    if (!ready) {
      return;
    }
    requestTriageInbox(worker);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready, syncOutcomeSeq]);

  return {
    submitCapture: (title: string, nowMs: number, fields: CaptureFields = {}) => {
      submitCaptureRequest(worker, title, nowMs, fields);
    },
  };
}
