import { useEffect } from "react";
import type { CoreStatus } from "../store/store";
import {
  requestBindings,
  requestQuestionSwitches,
  setBinding,
  setQuestionEnabled,
  type WorkerLike,
} from "../store/worker-client";

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
  /** #715: switches one standing question on or off. Does not wait for
   * `setQuestionEnabledResult` — `worker-client.ts` re-requests the switches
   * itself behind a successful one, the same shape `setBinding` uses. */
  setQuestionEnabled: (question: string, enabled: boolean) => void;
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
    // #715's switches ride the same refresh: they are `settings` rows too,
    // so a completed cycle is exactly when another device's toggle can have
    // arrived and when this device's own write stops being `pending`.
    requestQuestionSwitches(worker);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready, syncOutcomeSeq]);

  return {
    setBinding: (key: string, value: string) => {
      const nowMs = Date.now();
      const seed = mintBindingSeed(key, nowMs);
      setBinding(worker, seed, key, value, nowMs);
    },
    setQuestionEnabled: (question: string, enabled: boolean) => {
      const nowMs = Date.now();
      setQuestionEnabled(worker, mintQuestionSwitchSeed(question, nowMs), question, enabled, nowMs);
    },
  };
}

/** Mints this toggle's seed — [`mintBindingSeed`]'s twin, and deterministic
 * for the same reason: the `settings` row a question switch writes is
 * identified by the question itself, so retrying the identical intent (same
 * question, same `nowMs`) must reproduce the identical queue entry rather
 * than enqueue a second one. Distinct from a binding's seed by the
 * `:question:` infix, so the two vocabularies cannot collide on a key that
 * happens to share a name. */
export function mintQuestionSwitchSeed(question: string, nowMs: number): string {
  return `${question}:question:${nowMs}`;
}

/** Mints this binding write's seed. Deterministic — `client/core/src/
 * sync/mod.rs`'s seed-minting rule (#223): a binding write touches the
 * `settings` row `key` itself names (the key IS the entity's identity —
 * no id is ever minted from this seed's hash beyond the mutation's local
 * queue-entry id), so retrying the identical intent (same key, same
 * `nowMs`) must reproduce the identical entry rather than enqueue a
 * second one. Exported standalone, not inlined in the hook body, so it is
 * directly testable without rendering a component — `useCaptureWiring.ts`'s
 * own `mintSeed` split. */
export function mintBindingSeed(key: string, nowMs: number): string {
  return `${key}:binding:${nowMs}`;
}
