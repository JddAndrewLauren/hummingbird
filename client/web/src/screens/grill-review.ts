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
import type { GrillVerdictName, StepDTO, TaskStageName } from "../store/protocol";

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

/** #359 review round 1: whether confirming this verdict takes a STARTED
 * item off Now's frontier. `fog_remains` demotes every live stage to
 * Grilling (`hummingbird_domain::resulting_stage`'s own table — Triage,
 * Grilling, Ready, In Progress and Blocked all take that arm, the same fact
 * `wouldStrandPlan` above already reads off the verdict alone), but Ready
 * and In Progress are the only two of those actually visible on the
 * frontier today: Triage and Grilling were never on it, and Blocked is not
 * reachable from a Grill button at all (`item-actions.ts`'s `canGrill`).
 * The review card's own "Fog remains" badge names the verdict but not this
 * consequence — this is the deciding function for the sentence that does,
 * so the same "one deciding function" discipline applies here as it does to
 * `wouldStrandPlan` and every `item-actions.ts` export. */
export function demotesFromFrontier(verdict: GrillVerdictName, stage: TaskStageName): boolean {
  return verdict === "fog_remains" && (stage === "ready" || stage === "in_progress");
}

/** The sentence `demotesFromFrontier` gates — the actual consequence,
 * spelled out rather than left to the verdict badge's word alone. One
 * literal: both screens' review cards read the identical fact, since
 * neither Now nor Triage disagrees about what a Grilling stage move means. */
export const FRONTIER_DEMOTION_WARNING =
  "Confirming moves this item to Grilling and takes it off the frontier.";
