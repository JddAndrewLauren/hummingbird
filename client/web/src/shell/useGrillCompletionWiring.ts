import type { StepDTO } from "../store/protocol";
import { completeGrill, type GrillCompletion, type WorkerLike } from "../store/worker-client";

// #355/ADR-0023's Confirm mutation: `useTriageWiring.ts`'s own one-line
// wrapper shape, for `Core::complete_grill` instead of `Core::triage`. An
// ordinary queued mutation, deliberately outside the Grill turn lane
// (`useGrillWiring.ts`, `skills/no-queue.test.ts`) — see that hook's own
// module doc for why the two are two files rather than one.
export interface GrillCompletionWiring {
  /** Issues the mutation and hands back the `seed` it minted — the token
   * `TaskState.lastGrillCompletion` echoes, and the only thing that
   * identifies THIS confirm. The caller needs it because a Grill completion,
   * unlike a triage, has an open takeover waiting on its answer: which
   * broadcast is "mine" decides both when that takeover may close and whose
   * error its review card is allowed to render. */
  completeGrill: (
    itemId: string,
    sessionSteps: StepDTO[],
    completion: GrillCompletion,
  ) => string;
}

/** Mints this completion's seed, deterministic in the entity-minting sense
 * [`Core::complete_grill`]'s own doc names (a Grill is a brand-new record,
 * not a CAS write against an existing one) — `mintTriageSeed`'s own
 * reasoning, restated for this mutation's fields. */
export function mintGrillCompletionSeed(itemId: string, nowMs: number): string {
  return `${itemId}:complete-grill:${nowMs}`;
}

export function useGrillCompletionWiring(worker: WorkerLike): GrillCompletionWiring {
  function complete(itemId: string, sessionSteps: StepDTO[], completion: GrillCompletion): string {
    const nowMs = Date.now();
    const seed = mintGrillCompletionSeed(itemId, nowMs);
    completeGrill(worker, seed, itemId, sessionSteps, completion, nowMs);
    return seed;
  }

  return { completeGrill: complete };
}
