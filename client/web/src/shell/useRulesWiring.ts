import { useEffect } from "react";
import type { ConditionDTO, RuleDTO, TierName } from "../store/protocol";
import type { CoreStatus } from "../store/store";
import { createRule, patchRule, requestKindRegistry, requestRules, type WorkerLike } from "../store/worker-client";

// #140's rules wiring: requests the kind registry once (it never changes
// mid-session — the registry export needs no `Core` state at all) and the
// rule list once the core is ready and again on every completed cycle, the
// same "refresh once ready, then per-cycle" shape `useBindingsWiring.ts`
// uses and for the same reason — another device's rule edit rides the
// ordinary delta pull, and a completed cycle is when this device's own
// write stops being pending too.

export interface RulesWiring {
  createRule: (
    name: string,
    eventKind: string | null,
    conditions: ConditionDTO[],
    severity: string,
    tier: TierName,
    enabled: boolean,
  ) => void;
  patchRule: (
    current: RuleDTO,
    patch: {
      name?: string | null;
      eventKind?: string | null;
      conditions?: ConditionDTO[] | null;
      severity?: string | null;
      tier?: TierName | null;
      enabled?: boolean | null;
      /** Present at all = touched — see `worker-client.ts`'s `patchRule`.
       * Deleting a rule is this field on this same call. */
      deletedAt?: number | null;
    },
  ) => void;
}

export function useRulesWiring(
  worker: WorkerLike,
  status: CoreStatus,
  /** `TaskState.syncOutcomeSeq` — see `useFrontierWiring.ts`'s own doc for
   * why this, not the outcome's `kind`, is what a per-cycle refresh keys
   * on. */
  syncOutcomeSeq: number,
): RulesWiring {
  const ready = status === "ready";

  useEffect(() => {
    if (!ready) {
      return;
    }
    requestKindRegistry(worker);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready]);

  useEffect(() => {
    if (!ready) {
      return;
    }
    requestRules(worker);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready, syncOutcomeSeq]);

  return {
    createRule: (name, eventKind, conditions, severity, tier, enabled) => {
      const nowMs = Date.now();
      createRule(worker, mintRuleCreateSeed(), name, eventKind, conditions, severity, tier, enabled, nowMs);
    },
    patchRule: (current, patch) => {
      const nowMs = Date.now();
      patchRule(worker, mintRulePatchSeed(current.id, nowMs), current, patch, nowMs);
    },
  };
}

/** Mints a fresh, non-deterministic seed for a rule create — same
 * "creates a new entity" reasoning as `useCaptureWiring.ts`'s `mintSeed`:
 * this seed's hash becomes the new rule's id, so two creates in the same
 * millisecond must not collide into one rule. */
export function mintRuleCreateSeed(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `rule-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

/** Mints one rule patch's seed. Deterministic — `client/core/src/sync/mod.rs`'s
 * seed-minting rule (#223): a patch touches the rule `ruleId` itself
 * names, not a newly minted entity, so retrying the identical intent (same
 * rule, same `nowMs`) must reproduce the identical queue entry rather than
 * enqueue a second one — `mintBindingSeed`'s own contract, ported. */
export function mintRulePatchSeed(ruleId: string, nowMs: number): string {
  return `${ruleId}:patch:${nowMs}`;
}
