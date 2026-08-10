import { describe, expect, it } from "vitest";
import type { ProjectDTO, TaskItemDTO } from "../store/protocol";
import { groupByProject } from "./frontier-groups";

function item(overrides: Partial<TaskItemDTO> = {}): TaskItemDTO {
  return {
    id: "id-0",
    seq: null,
    title: "untitled",
    description: null,
    stage: "ready",
    size: null,
    energy: null,
    context: null,
    priority: 0,
    projectId: null,
    projectPos: null,
    deadline: null,
    scheduledDate: null,
    source: null,
    sourceKey: null,
    sourceUrl: null,
    archivedAt: null,
    createdAt: 1,
    updatedAt: 1,
    version: 0,
    pending: false,
    ...overrides,
  };
}

function project(overrides: Partial<ProjectDTO> = {}): ProjectDTO {
  return {
    id: "p-0",
    name: "untitled project",
    archivedAt: null,
    createdAt: 1,
    updatedAt: 1,
    version: 0,
    ...overrides,
  };
}

describe("groupByProject", () => {
  it("groups items under their projectId, preserving each group's given order", () => {
    const a = item({ id: "a", projectId: "p-1" });
    const b = item({ id: "b", projectId: "p-2" });
    const c = item({ id: "c", projectId: "p-1" });

    const groups = groupByProject([a, b, c], []);

    expect(groups).toEqual([
      { projectId: "p-1", projectName: null, items: [a, c] },
      { projectId: "p-2", projectName: null, items: [b] },
    ]);
  });

  it("resolves a group's real name from the given project list", () => {
    const a = item({ id: "a", projectId: "p-1" });
    const projects = [project({ id: "p-1", name: "Ship the release" })];

    const groups = groupByProject([a], projects);

    expect(groups).toEqual([{ projectId: "p-1", projectName: "Ship the release", items: [a] }]);
  });

  it("falls back to a null name for a projectId not (yet) in the project list", () => {
    const a = item({ id: "a", projectId: "unknown-id" });

    const groups = groupByProject([a], [project({ id: "p-1", name: "Ship the release" })]);

    expect(groups).toEqual([{ projectId: "unknown-id", projectName: null, items: [a] }]);
  });

  it("buckets items with no project under a null projectId group", () => {
    const withProject = item({ id: "a", projectId: "p-1" });
    const withoutProject = item({ id: "b", projectId: null });

    const groups = groupByProject([withProject, withoutProject], []);

    expect(groups).toEqual([
      { projectId: "p-1", projectName: null, items: [withProject] },
      { projectId: null, projectName: null, items: [withoutProject] },
    ]);
  });

  it("the unassigned group sorts last even when it appears first in the input", () => {
    const withoutProject = item({ id: "a", projectId: null });
    const withProject = item({ id: "b", projectId: "p-1" });

    const groups = groupByProject([withoutProject, withProject], []);

    expect(groups.map((g) => g.projectId)).toEqual(["p-1", null]);
  });

  it("returns no groups for an empty list", () => {
    expect(groupByProject([], [])).toEqual([]);
  });

  it("never mutates its input", () => {
    const input = [item({ id: "a", projectId: "p-1" })];
    const copy = [...input];

    groupByProject(input, []);

    expect(input).toEqual(copy);
  });
});
