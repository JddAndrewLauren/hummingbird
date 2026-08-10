// The frontier's "grouped by project" display shape (issue #108's "What to
// build"). Groups by `projectId`, resolving each group's real `name` from
// the project list `getProjects` returns (`Core::projects`) — PR #200's
// review corrected an earlier version of this module that had no such wire
// hop and rendered a raw project uuid as the section heading.

import type { ProjectDTO, TaskItemDTO } from "../store/protocol";

export interface FrontierGroup {
  projectId: string | null;
  /** The project's real name, resolved from `projects` — `null` when
   * `projectId` is `null` (no project) or when it names a project not (yet)
   * present in `projects` (a project the frontier answer references but a
   * `getProjects` answer hasn't caught up to yet, or one this device has
   * never seen). `NowScreen` is the one place that turns `null` into
   * display text, so this module stays a pure data shape. */
  projectName: string | null;
  items: TaskItemDTO[];
}

/** Stable grouping: each group keeps the input order of the items that
 * fall into it, groups appear in first-seen order, and the unassigned
 * (`projectId: null`) group always sorts last regardless of where its
 * items appeared in the input — an unassigned item should never visually
 * outrank a real project's section. */
export function groupByProject(
  items: readonly TaskItemDTO[],
  projects: readonly ProjectDTO[],
): FrontierGroup[] {
  const namesById = new Map(projects.map((project) => [project.id, project.name]));

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
    projectName: projectId === null ? null : (namesById.get(projectId) ?? null),
    items: byProject.get(projectId) ?? [],
  }));
}
