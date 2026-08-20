import type { WorkerLike } from "../store/worker-client";
import { createProject } from "../store/worker-client";

// #624's projects wiring: the Projects screen's one write.
//
// **It deliberately owns no refresher.** `useFrontierWiring.ts` already calls
// `requestProjects` once the core is ready and again on every completed
// cycle, app-wide — the frontier's "grouped by project" display has needed
// that read since #108. A second per-cycle requester here would be a second
// clock for one read, which is the thing this repo bans outright (CLAUDE.md's
// "No competing clocks"). So this hook is a write door only, and the grid's
// freshness is somebody else's already-solved problem.
//
// Nothing here is optimistic: `createProject` enqueues, and the project
// appears in `TaskState.projects` only when a completed cycle pulls it back
// (`Core::create_project`'s own doc). The caller says it is waiting in the
// meantime, keyed on the *minted id* in `lastProjectWrite` — not on the seed,
// which this hook mints and drops. `lastProjectWrite` is one broadcast slot
// shared by every connected view (`protocol.ts`), so a second tab's create
// briefly moves this tab's waiting line; that is `RulesScreen`'s behaviour
// too (`lastRuleWrite`), and closing the gap is a change to both surfaces at
// once rather than a private one here.

export interface ProjectsWiring {
  createProject: (name: string) => void;
}

export function useProjectsWiring(worker: WorkerLike): ProjectsWiring {
  return {
    createProject: (name) => {
      createProject(worker, mintProjectCreateSeed(), name, Date.now());
    },
  };
}

/** Mints a fresh, non-deterministic seed for a project create — same "creates
 * a new entity" reasoning as `mintRuleCreateSeed`: this seed's hash becomes
 * the new project's id, so two creates in the same millisecond must not
 * collide into one project. */
export function mintProjectCreateSeed(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `project-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}
