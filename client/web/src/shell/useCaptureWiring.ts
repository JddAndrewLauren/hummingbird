import { useEffect } from "react";
import type { CoreStatus } from "../store/store";
import { captureTask, requestTriageInbox, type WorkerLike } from "../store/worker-client";

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
  submitCapture: (title: string, nowMs: number) => void;
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
    submitCapture: (title: string, nowMs: number) => {
      captureTask(worker, mintSeed(), title, "triage", nowMs);
    },
  };
}
