// The component-test seam (G2). Every `*.test.tsx` in this app imports its
// render/query/event helpers from here rather than from
// `@testing-library/react` directly, for one reason worth the indirection:
// React Testing Library's automatic `cleanup` only registers itself when a
// global `afterEach` exists, and this repo runs vitest WITHOUT
// `globals: true`. Imported straight, RTL would silently skip cleanup and
// every test after the first would query a DOM still holding the previous
// test's tree — a failure mode that looks like a flaky assertion, not a
// missing teardown. Registering it explicitly here makes it real.
//
// This file is also the answer to "why component tests in a repo this
// lean". Three of the four PRs in the S10-S13 batch were rejected on first
// review for shipping UI state with NO READER: `requestIsPending` had no
// caller, a capture never re-read the triage inbox, and a blocked item's
// action row was permanently `disabled`. All three compiled, typechecked,
// passed their pure-module unit tests, and did nothing at runtime.
// `pnpm typecheck` cannot see that something has no caller. Mounting it can.
//
// Keep the surface small: render, screen, fireEvent, and the DTO builders
// below. Anything richer (user-event, jest-dom matchers) is a new dependency
// and should be argued for, not drifted into.

import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";
import type {
  BlockedFrontierEntryDTO,
  ProjectDTO,
  TaskItemDTO,
} from "../store/protocol";
import type { TaskState } from "../store/store";

export { render, screen, fireEvent, within, act } from "@testing-library/react";

afterEach(() => {
  cleanup();
});

/** A complete `TaskItemDTO` with every field at a boring default, so a test
 * states only the fields it is actually about. Spelled out rather than cast
 * from a partial: a real field added to the DTO must break this builder and
 * make someone choose a default, which an `as TaskItemDTO` would hide. */
export function itemDTO(overrides: Partial<TaskItemDTO> = {}): TaskItemDTO {
  return {
    id: "item-1",
    seq: 1,
    title: "An action",
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
    createdAt: 1_000,
    updatedAt: 1_000,
    version: 1,
    pending: false,
    ...overrides,
  };
}

export function projectDTO(overrides: Partial<ProjectDTO> = {}): ProjectDTO {
  return {
    id: "project-1",
    name: "A project",
    archivedAt: null,
    createdAt: 1_000,
    updatedAt: 1_000,
    version: 1,
    ...overrides,
  };
}

export function blockedEntryDTO(
  item: TaskItemDTO,
  blockedBy: TaskItemDTO[],
): BlockedFrontierEntryDTO {
  return { item, blockedBy };
}

/** A `TaskState` at rest — the same shape `store.ts`'s `initialTaskState`
 * starts at. Deliberately a separate literal from that private constant: a
 * test asserting over an empty frontier should keep passing if the store's
 * own initial value ever changes for an unrelated reason, and should fail
 * loudly if a required field is added. */
export function taskState(overrides: Partial<TaskState> = {}): TaskState {
  return {
    frontier: [],
    triageInbox: [],
    blocked: [],
    stepsByItem: {},
    projects: [],
    pending: {},
    lastCapture: null,
    lastAct: null,
    lastTriage: null,
    lastSyncOutcome: null,
    lastSyncAtMs: null,
    syncOutcomeSeq: 0,
    queueDepth: null,
    deadLetters: [],
    needsReconnect: false,
    hostError: null,
    ...overrides,
  };
}
