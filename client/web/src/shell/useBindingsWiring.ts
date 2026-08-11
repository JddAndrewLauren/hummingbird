import { useEffect } from "react";
import type { CoreStatus } from "../store/store";
import { requestBindings, setBinding, type WorkerLike } from "../store/worker-client";

// #118's bindings wiring: requests every standing-question binding once the
// core is ready, and again after every sync cycle — the same "refresh once
// ready, then per-cycle" shape `useFrontierWiring.ts` uses, and for the same
// reason. A completed cycle is exactly when a binding another device set can
// have arrived (they ride the ordinary delta pull), and it is also when this
// device's own write stops being `pending`.
//
// Thin glue, no clock of its own beyond the one `Date.now` call every
// mutation entry point needs for its own `nowMs`/seed — ADR-0007's single
// 60-second interval lives in the SharedWorker, and nothing here schedules
// anything.

export interface BindingsWiring {
  /** Sends one binding write. The caller decides whether the draft is worth
   * sending (`screens/bindings.ts`'s `canSubmitBinding`); this trusts it and
   * enqueues, exactly as `useCaptureWiring` does. Does not wait for
   * `setBindingResult` — `worker-client.ts` re-requests the bindings itself
   * the moment a successful one broadcasts. */
  setBinding: (key: string, value: string) => void;
}

export function useBindingsWiring(
  worker: WorkerLike,
  status: CoreStatus,
  /** `TaskState.syncOutcomeSeq` — see `useFrontierWiring.ts`'s own parameter
   * doc for why this, not the outcome's `kind`, is what a per-cycle refresh
   * must key on. */
  syncOutcomeSeq: number,
): BindingsWiring {
  const ready = status === "ready";

  useEffect(() => {
    if (!ready) {
      return;
    }
    requestBindings(worker);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready, syncOutcomeSeq]);

  return {
    setBinding: (key: string, value: string) => {
      const nowMs = Date.now();
      const seed = mintBindingSeed(key, nowMs);
      setBinding(worker, seed, key, value, nowMs);
    },
  };
}

/** Mints this binding write's seed. Deterministic — `client/core/src/
 * sync/mod.rs`'s seed-minting rule (#223): a binding write touches a
 * `settings` row keyed by `key`, which either already exists or is created
 * idempotently at `expected_version: 0`, so retrying the identical intent
 * (same key, same `nowMs`) must reproduce the identical seed. Exported
 * standalone, not inlined in the hook body, so it is directly testable
 * without rendering a component — `useCaptureWiring.ts`'s own `mintSeed`
 * split. */
export function mintBindingSeed(key: string, nowMs: number): string {
  return `${key}:binding:${nowMs}`;
}
