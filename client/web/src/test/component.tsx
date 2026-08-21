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
  BindingDTO,
  BlockedFrontierEntryDTO,
  FogDTO,
  LedgerRowDTO,
  PaneReadDTO,
  PaneSnapshotDTO,
  ProjectDTO,
  ProjectLinkDTO,
  RouteDTO,
  StepDTO,
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

/** A complete `StepDTO` at a boring default: live, unticked, first in
 * position. Spelled out for the same reason `itemDTO` is. */
export function stepDTO(overrides: Partial<StepDTO> = {}): StepDTO {
  return {
    id: "step-1",
    itemId: "item-1",
    body: "put on music",
    done: false,
    position: 1,
    deletedAt: null,
    version: 1,
    ...overrides,
  };
}

/** One `BindingDTO` at a boring default (#118): a known, unset, not-pending
 * `race-series`. Spelled out for the same reason `itemDTO` is. */
export function bindingDTO(overrides: Partial<BindingDTO> = {}): BindingDTO {
  return {
    key: "race-series",
    known: true,
    pending: false,
    value: { state: "unset" },
    ...overrides,
  };
}

/** One `PaneSnapshotDTO` (#245) at a boring default: a well-formed
 * `city-waste/v2` envelope, fetched 40 minutes ago against a daily cadence.
 * `body` is a JSON *string*, exactly as it crosses the seam. */
export function paneSnapshotDTO(overrides: Partial<PaneSnapshotDTO> = {}): PaneSnapshotDTO {
  return {
    key: "collection",
    fetchedAtMs: 1_000,
    envelope: {
      kind: "ok",
      schema: "city-waste/v2",
      polledEveryMs: 86_400_000,
      body: wasteBody(),
    },
    freshness: { kind: "age", ageMs: 40 * 60_000, declaredCadenceMs: 86_400_000 },
    ...overrides,
  };
}

/** One `PaneReadDTO` (#245) at a boring default: one good snapshot, no live
 * alerts. Spelled out for the same reason `itemDTO` is. */
export function paneReadDTO(overrides: Partial<PaneReadDTO> = {}): PaneReadDTO {
  return {
    source: "city-waste/v2",
    snapshots: [paneSnapshotDTO()],
    liveAlerts: [],
    ...overrides,
  };
}

/** A `city-waste/v2` payload body as its JSON text — the shape
 * `screens/waste-pane/waste.ts` parses. Whole-day civil dates plus the IANA
 * zone they are civil *in*, since the address's day is what a collection
 * happens on, never the device's. */
export function wasteBody(
  overrides: {
    zone?: string;
    scheduled?: string;
    collectedOn?: string;
    streams?: string[];
  } = {},
): string {
  return JSON.stringify({
    zone: overrides.zone ?? "America/Los_Angeles",
    scheduled: overrides.scheduled ?? "2026-08-17",
    collected_on: overrides.collectedOn ?? "2026-08-17",
    streams: overrides.streams ?? ["trash", "yard"],
  });
}

/** A complete `LedgerRowDTO` — `itemDTO`'s defaults plus the row facts at
 * their boring values (live, no badges). Spelled out for the same reason. */
export function ledgerRowDTO(overrides: Partial<LedgerRowDTO> = {}): LedgerRowDTO {
  return {
    ...itemDTO(),
    absentSinceMs: null,
    deadLettered: false,
    hasLiveAlert: false,
    ...overrides,
  };
}

export function projectDTO(overrides: Partial<ProjectDTO> = {}): ProjectDTO {
  return {
    id: "project-1",
    name: "A project",
    githubRepo: null,
    defaultContext: null,
    archivedAt: null,
    createdAt: 1_000,
    updatedAt: 1_000,
    version: 1,
    ...overrides,
  };
}

export function projectLinkDTO(overrides: Partial<ProjectLinkDTO> = {}): ProjectLinkDTO {
  return {
    id: "link-1",
    projectId: "project-1",
    url: "https://example.com",
    label: null,
    position: 1,
    removedAt: null,
    version: 1,
    ...overrides,
  };
}

export function routeDTO(overrides: Partial<RouteDTO> = {}): RouteDTO {
  return {
    projectId: "project-1",
    destination: null,
    notes: null,
    updatedAt: 1_000,
    version: 1,
    ...overrides,
  };
}

export function fogDTO(overrides: Partial<FogDTO> = {}): FogDTO {
  return {
    id: "fog-1",
    projectId: "project-1",
    question: "What permit does this need?",
    position: 0,
    resolvedAt: null,
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
    grillingItems: [],
    blocked: [],
    stepsByItem: {},
    projects: null,
    archivedProjects: null,
    ledger: null,
    search: null,
    done: null,
    bindings: null,
    kindRegistry: null,
    rules: null,
    lastRuleWrite: null,
    lastProjectWrite: null,
    linksByProject: {},
    lastProjectLinkWrite: null,
    routeByProject: {},
    lastRouteWrite: null,
    fogByProject: {},
    lastFogWrite: null,
    actionsByProject: {},
    lastActionReorder: null,
    lastStepWrite: null,
    paneReads: {},
    pending: {},
    lastCapture: null,
    lastAct: null,
    lastTriage: null,
    lastGrillCompletion: null,
    lastGrillDraftWrite: null,
    grillDraftItemIds: [],
    grillDraftByItem: {},
    lastBindingWrite: null,
    lastSyncOutcome: null,
    lastSyncAtMs: null,
    lastSuccessfulSyncAtMs: null,
    syncOutcomeSeq: 0,
    queueDepth: null,
    deadLetters: [],
    needsReconnect: false,
    hostError: null,
    ...overrides,
  };
}
