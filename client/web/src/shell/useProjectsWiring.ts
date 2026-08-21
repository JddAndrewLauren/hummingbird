import type { ProjectDTO, ProjectLinkDTO } from "../store/protocol";
import type { WorkerLike } from "../store/worker-client";
import {
  createProject,
  createProjectLink,
  patchProject,
  patchProjectLink,
  requestProjectLinks,
} from "../store/worker-client";

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
  /** #625's properties-card write: `patch` carries only the fields the card
   * actually changed, `undefined` for the rest — [`patchProject`]'s own
   * "leave this alone" contract, unchanged across this hook. */
  patchProject: (
    current: ProjectDTO,
    patch: { githubRepo?: string | null; defaultContext?: string | null },
  ) => void;
  /** #626's per-project link read. Unlike `createProject`/`patchProject`
   * above, this is a read door, not a write — but it stays here rather than
   * joining `useFrontierWiring`'s app-wide refresh because it is scoped to
   * *one open dossier*, which only the dossier itself (a local `useState`
   * this hook does not own) knows about. The caller — the dossier's own
   * effect — decides when to call it (on open, and again on every completed
   * cycle it is still open for); this hook still owns no clock of its own. */
  requestProjectLinks: (projectId: string) => void;
  /** #626's link create: the dossier aside's add-a-link gesture. `position`
   * is the caller's to compute (append to the end of the list it already
   * has) — this hook mints no ordering of its own. */
  createProjectLink: (projectId: string, url: string, label: string | null, position: number) => void;
  /** #626's link patch: editing, reordering and removing a link, all
   * through this one entry point — same "leave this alone" contract
   * `patchProject` carries. */
  patchProjectLink: (
    current: ProjectLinkDTO,
    patch: { url?: string; label?: string | null; position?: number; removedAt?: number | null },
  ) => void;
}

export function useProjectsWiring(worker: WorkerLike): ProjectsWiring {
  return {
    createProject: (name) => {
      createProject(worker, mintProjectCreateSeed(), name, Date.now());
    },
    patchProject: (current, patch) => {
      const nowMs = Date.now();
      patchProject(worker, mintProjectPatchSeed(current.id, nowMs), current, patch, nowMs);
    },
    requestProjectLinks: (projectId) => {
      requestProjectLinks(worker, projectId);
    },
    createProjectLink: (projectId, url, label, position) => {
      const nowMs = Date.now();
      createProjectLink(worker, mintProjectLinkCreateSeed(), projectId, url, label, position, nowMs);
    },
    patchProjectLink: (current, patch) => {
      const nowMs = Date.now();
      patchProjectLink(worker, mintProjectLinkPatchSeed(current.id, nowMs), current, patch, nowMs);
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

/** Mints one project patch's seed (#625). Deterministic —
 * `mintRulePatchSeed`'s own contract, ported: a patch touches the project
 * `projectId` itself names, not a newly minted entity, so retrying the
 * identical intent (same project, same `nowMs`) must reproduce the identical
 * queue entry rather than enqueue a second one. */
export function mintProjectPatchSeed(projectId: string, nowMs: number): string {
  return `${projectId}:patch:${nowMs}`;
}

/** Mints a fresh, non-deterministic seed for a link create (#626) — same
 * "creates a new entity" reasoning as [`mintProjectCreateSeed`]. */
export function mintProjectLinkCreateSeed(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `project-link-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

/** Mints one link patch's seed (#626). Deterministic, same
 * [`mintProjectPatchSeed`] reasoning: a patch touches the link `linkId`
 * itself names, so retrying the identical intent must reproduce the
 * identical queue entry rather than enqueue a second one. */
export function mintProjectLinkPatchSeed(linkId: string, nowMs: number): string {
  return `${linkId}:patch:${nowMs}`;
}
