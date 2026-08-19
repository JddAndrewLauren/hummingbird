// The review card's plan-replacement tick (#355, ADR-0023) and the
// frontier-demotion warning (#359). Sunk to
// `hummingbird_core::decisions::skills::review` at #539 (ADR-0025) — this
// module is now a thin wrapper over the seam, kept so every existing caller
// and test is unchanged.
//
// `verdict === "fog_remains"` is enough to know the confirm will strand a
// live plan, with no second stage-mapping needed here: every live stage
// demotes to Grilling on `fog_remains`
// (`hummingbird_domain::resulting_stage`'s own table — Triage, Grilling,
// Ready, In Progress and Blocked all take that arm), so reading the
// verdict the model itself proposed is enough to know a demotion is
// coming, without this file re-deriving what stage results. `resolved`
// never strands anything: it either promotes Triage/Grilling to Ready or
// leaves an already-live stage exactly where it was, both of which keep
// whatever plan the item carries exactly as relevant as before.

import {
  grillDemotesFromFrontierFromCore,
  grillPlanReplacementLabelFromCore,
  grillWouldStrandPlanFromCore,
} from "../decisions/seam";
import type { GrillVerdictName, StepDTO, TaskStageName } from "../store/protocol";

/** Whether confirming this verdict risks stranding a live plan: the item
 * has at least one live, undone Step, and the verdict is `fog_remains` (the
 * only arm that ever demotes a live stage to Grilling). `false` for
 * `resolved`, and `false` for an item with no plan to protect either way —
 * the tick has nothing to offer when there is nothing it could delete. */
export function wouldStrandPlan(verdict: GrillVerdictName, steps: StepDTO[]): boolean {
  return grillWouldStrandPlanFromCore(verdict, steps);
}

/** The tick's own label, naming the count — never a bare "Delete steps?"
 * that leaves the person guessing what they are agreeing to. */
export function planReplacementLabel(steps: StepDTO[]): string {
  return grillPlanReplacementLabelFromCore(steps);
}

/** #359 review round 1: whether confirming this verdict takes a STARTED
 * item off Now's frontier. `fog_remains` demotes every live stage to
 * Grilling, but Ready and In Progress are the only two of those actually
 * visible on the frontier today: Triage and Grilling were never on it, and
 * Blocked is not reachable from a Grill button at all
 * (`item-actions.ts`'s `canGrill`). */
export function demotesFromFrontier(verdict: GrillVerdictName, stage: TaskStageName): boolean {
  return grillDemotesFromFrontierFromCore(verdict, stage);
}

/** The sentence `demotesFromFrontier` gates — the actual consequence,
 * spelled out rather than left to the verdict badge's word alone. A
 * module-evaluation-time literal, pinned against
 * `hummingbird_core::decisions::skills::FRONTIER_DEMOTION_WARNING` by
 * `seam.test.ts`, the same reason `skills/decline.ts`'s three constants
 * stay literal (`seam.ts`'s own header). */
export const FRONTIER_DEMOTION_WARNING =
  "Confirming moves this item to Grilling and takes it off the frontier.";
