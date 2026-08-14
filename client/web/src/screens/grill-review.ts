// The review card's plan-replacement tick (#355, ADR-0023): the explicit,
// default-off gesture that lets a Confirm carry `Core::complete_grill`'s
// `delete_unticked_plan`, naming the step count it would delete.
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
// whatever plan the item carries exactly as relevant as before. This is
// the brief's own rule ("Stage after confirming comes from the
// hummingbird-domain verdict function, never a UI branch") read for what
// it protects: the *actual* stage move is `Core::complete_grill`'s alone;
// this module only decides whether to *ask about* a plan, from a fact the
// model already named.

import { liveUndoneSteps } from "../skills/microtask-affordance";
import type { GrillVerdictName, StepDTO } from "../store/protocol";

/** Whether confirming this verdict risks stranding a live plan: the item
 * has at least one live, undone Step, and the verdict is `fog_remains` (the
 * only arm that ever demotes a live stage to Grilling). `false` for
 * `resolved`, and `false` for an item with no plan to protect either way —
 * the tick has nothing to offer when there is nothing it could delete. */
export function wouldStrandPlan(verdict: GrillVerdictName, steps: StepDTO[]): boolean {
  return verdict === "fog_remains" && liveUndoneSteps(steps).length > 0;
}

/** The tick's own label, naming the count — never a bare "Delete steps?"
 * that leaves the person guessing what they are agreeing to. Singular is
 * spelled out rather than left to an `s` suffix rule with one exception, the
 * same discipline `ItemDetailPanel`'s own step-count label already uses. */
export function planReplacementLabel(steps: StepDTO[]): string {
  const count = liveUndoneSteps(steps).length;
  return `Also delete ${count} unfinished step${count === 1 ? "" : "s"}`;
}
