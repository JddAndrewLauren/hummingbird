// PROTOTYPE (#449) — throwaway. In-memory state + the mutation surface the
// real Projects screen will need (C1's list, roughly). No persistence, no
// sync, no optimistic-overlay questions — state lives in one useState and
// survives variant switches so a flow started in one layout can be judged
// in another.

import { useState } from "react";
import { CONTEXTS, seedProjects, liveActions } from "./fixture";
import type { ProtoProject } from "./fixture";

let counter = 0;
function freshId(prefix: string): string {
  counter += 1;
  return `${prefix}-new-${counter}`;
}

export interface WorldApi {
  /** Round-trips ~600ms like v1 will (no optimistic projection for
   * projects); resolves with the new project's id. */
  createProject(name: string): Promise<string>;
  patchProject(
    id: string,
    patch: Partial<Pick<ProtoProject, "name" | "githubRepo" | "defaultContext">>,
  ): void;
  archiveProject(id: string): void;
  unarchiveProject(id: string): void;
  setRoute(id: string, patch: Partial<{ destination: string; notes: string }>): void;
  addFog(id: string, question: string): void;
  editFog(id: string, fogId: string, question: string): void;
  resolveFog(id: string, fogId: string, resolved: boolean): void;
  addLink(id: string, url: string, label: string): void;
  removeLink(id: string, linkId: string): void;
  /** Reorder among live actions: move one up or down a slot. */
  moveAction(id: string, actionId: string, direction: -1 | 1): void;
  tickStep(id: string, actionId: string, stepId: string): void;
  addStep(id: string, actionId: string, body: string): void;
  editStep(id: string, actionId: string, stepId: string, body: string): void;
  deleteStep(id: string, actionId: string, stepId: string): void;
}

/** What every variant receives — the world, its mutations, and a shared
 * selection so a project opened in one layout stays open in the next. */
export interface VariantProps {
  projects: ProtoProject[];
  api: WorldApi;
  selectedId: string | null;
  onSelect: (id: string | null) => void;
}

export function usePrototypeWorld(): [ProtoProject[], WorldApi] {
  const [projects, setProjects] = useState<ProtoProject[]>(seedProjects);

  function patch(id: string, map: (project: ProtoProject) => ProtoProject) {
    setProjects((current) =>
      current.map((project) => (project.id === id ? map(project) : project)),
    );
  }

  const api: WorldApi = {
    createProject(name) {
      const id = freshId("proj");
      return new Promise((resolve) => {
        setTimeout(() => {
          setProjects((current) => [
            ...current,
            {
              id,
              name,
              githubRepo: null,
              defaultContext: null,
              archivedAt: null,
              route: { destination: "", notes: "" },
              fog: [],
              links: [],
              actions: [],
            },
          ]);
          resolve(id);
        }, 600);
      });
    },

    patchProject(id, fields) {
      patch(id, (project) => ({ ...project, ...fields }));
    },

    // The timestamp-matched cascade, exactly as the plan states it: archive
    // stamps every live item with the project's own stamp; unarchive clears
    // only items whose stamp matches, so what was archived on its own stays
    // archived.
    archiveProject(id) {
      const stamp = Date.now();
      patch(id, (project) => ({
        ...project,
        archivedAt: stamp,
        actions: project.actions.map((action) =>
          action.archivedAt === null ? { ...action, archivedAt: stamp } : action,
        ),
      }));
    },

    unarchiveProject(id) {
      patch(id, (project) => ({
        ...project,
        archivedAt: null,
        actions: project.actions.map((action) =>
          action.archivedAt === project.archivedAt ? { ...action, archivedAt: null } : action,
        ),
      }));
    },

    setRoute(id, fields) {
      patch(id, (project) => ({ ...project, route: { ...project.route, ...fields } }));
    },

    addFog(id, question) {
      patch(id, (project) => ({
        ...project,
        fog: [...project.fog, { id: freshId("fog"), question, resolvedAt: null }],
      }));
    },

    editFog(id, fogId, question) {
      patch(id, (project) => ({
        ...project,
        fog: project.fog.map((entry) => (entry.id === fogId ? { ...entry, question } : entry)),
      }));
    },

    resolveFog(id, fogId, resolved) {
      patch(id, (project) => ({
        ...project,
        fog: project.fog.map((entry) =>
          entry.id === fogId ? { ...entry, resolvedAt: resolved ? Date.now() : null } : entry,
        ),
      }));
    },

    addLink(id, url, label) {
      patch(id, (project) => ({
        ...project,
        links: [...project.links, { id: freshId("lnk"), url, label: label || null }],
      }));
    },

    removeLink(id, linkId) {
      patch(id, (project) => ({
        ...project,
        links: project.links.filter((link) => link.id !== linkId),
      }));
    },

    moveAction(id, actionId, direction) {
      patch(id, (project) => {
        const live = liveActions(project);
        const from = live.findIndex((action) => action.id === actionId);
        const to = from + direction;
        if (from < 0 || to < 0 || to >= live.length) return project;
        const reordered = [...live];
        const [moved] = reordered.splice(from, 1);
        reordered.splice(to, 0, moved);
        // Splice the reordered live set back around any archived rows.
        let cursor = 0;
        return {
          ...project,
          actions: project.actions.map((action) =>
            action.archivedAt === null ? reordered[cursor++] : action,
          ),
        };
      });
    },

    tickStep(id, actionId, stepId) {
      patch(id, (project) => ({
        ...project,
        actions: project.actions.map((action) =>
          action.id === actionId
            ? {
                ...action,
                steps: action.steps.map((step) =>
                  step.id === stepId ? { ...step, done: !step.done } : step,
                ),
              }
            : action,
        ),
      }));
    },

    addStep(id, actionId, body) {
      patch(id, (project) => ({
        ...project,
        actions: project.actions.map((action) =>
          action.id === actionId
            ? { ...action, steps: [...action.steps, { id: freshId("stp"), body, done: false }] }
            : action,
        ),
      }));
    },

    editStep(id, actionId, stepId, body) {
      patch(id, (project) => ({
        ...project,
        actions: project.actions.map((action) =>
          action.id === actionId
            ? {
                ...action,
                steps: action.steps.map((step) => (step.id === stepId ? { ...step, body } : step)),
              }
            : action,
        ),
      }));
    },

    deleteStep(id, actionId, stepId) {
      patch(id, (project) => ({
        ...project,
        actions: project.actions.map((action) =>
          action.id === actionId
            ? { ...action, steps: action.steps.filter((step) => step.id !== stepId) }
            : action,
        ),
      }));
    },
  };

  return [projects, api];
}

export { CONTEXTS };
