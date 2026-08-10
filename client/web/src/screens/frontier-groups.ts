// The frontier's "grouped by project" display shape (issue #108's "What to
// build"). Groups by `projectId` only — the web binding has no `getProjects`
// wire request yet (nothing in `store/protocol.ts` surfaces a project's
// *name* to this side), so a project's own group is labelled by its id for
// now; `NowScreen` renders it as "Project <id>" until a later issue adds
// project-name resolution. Posted as a finding on issue #108 rather than
// guessed at here.

import type { TaskItemDTO } from "../store/protocol";

export interface FrontierGroup {
  projectId: string | null;
  items: TaskItemDTO[];
}

/** Stable grouping: each group keeps the input order of the items that
 * fall into it, groups appear in first-seen order, and the unassigned
 * (`projectId: null`) group always sorts last regardless of where its
 * items appeared in the input — an unassigned item should never visually
 * outrank a real project's section. */
export function groupByProject(items: readonly TaskItemDTO[]): FrontierGroup[] {
  const order: (string | null)[] = [];
  const byProject = new Map<string | null, TaskItemDTO[]>();

  for (const item of items) {
    const key = item.projectId;
    let bucket = byProject.get(key);
    if (!bucket) {
      bucket = [];
      byProject.set(key, bucket);
      order.push(key);
    }
    bucket.push(item);
  }

  const withProject = order.filter((key): key is string => key !== null);
  const unassigned = order.includes(null) ? [null] : [];

  return [...withProject, ...unassigned].map((projectId) => ({
    projectId,
    items: byProject.get(projectId) ?? [],
  }));
}
