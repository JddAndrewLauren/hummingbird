import { describe, expect, it } from "vitest";
import type {
  CalendarWorkerRequest,
  DiagnosticsWorkerRequest,
  SyncCadenceRequest,
  TaskWorkerRequest,
} from "../store/protocol";
import {
  isDiagnosticsWorkerRequest,
  isSyncCadenceRequest,
  isTaskWorkerRequest,
} from "./request-router";

describe("isTaskWorkerRequest", () => {
  it.each<TaskWorkerRequest>([
    { type: "pushTaskApiKey", apiKey: "k" },
    { type: "initTaskApiKey", apiKey: "k" },
    { type: "clearTaskApiKey" },
    {
      type: "capture",
      seed: "s",
      title: "t",
      stage: "triage",
      fields: {
        size: null,
        energy: null,
        context: null,
        description: null,
        projectId: null,
        priority: null,
        deadline: null,
        scheduledDate: null,
        linkUrl: null,
        linkLabel: null,
      },
      nowMs: 1,
    },
    { type: "act", seed: "s", itemId: "i", action: "start", nowMs: 1 },
    { type: "triage", seed: "s", itemId: "i", destination: "ready", edits: {}, nowMs: 1 },
    { type: "getPaneRead", source: "city-waste/v2", nowMs: 1 },
    { type: "getFrontier" },
    { type: "getTriageInbox" },
    { type: "getGrillingItems" },
    { type: "getBlocked" },
    { type: "getSteps", itemId: "i" },
    { type: "getProjects" },
    { type: "isPending", itemId: "i" },
    { type: "runSync", nowMs: 1, trigger: "user", forceFullSweep: false, jitterUnit: 0 },
    { type: "getQueueDepth" },
    { type: "getDeadLetters" },
    { type: "getMirrorSnapshot" },
  ])("routes every task request type ($type) to the task queue", (request) => {
    expect(isTaskWorkerRequest(request)).toBe(true);
  });

  it.each<CalendarWorkerRequest>([
    { type: "pushToken", token: "t" },
    { type: "setCalendarSelections", selections: [] },
    { type: "pollStart", nowMs: 1 },
    { type: "pollRefresh", nowMs: 1 },
    { type: "pollTimer", nowMs: 1 },
    { type: "listCalendars" },
  ])("routes every calendar request type ($type) to the calendar queue", (request) => {
    expect(isTaskWorkerRequest(request)).toBe(false);
  });

  it.each<SyncCadenceRequest>([
    { type: "setViewVisibility", hidden: true },
    { type: "syncFocusTrigger" },
    { type: "manualSyncTrigger" },
  ])("never routes a sync-cadence request ($type) to the task queue", (request) => {
    expect(isTaskWorkerRequest(request)).toBe(false);
  });
});

describe("isSyncCadenceRequest", () => {
  it.each<SyncCadenceRequest>([
    { type: "setViewVisibility", hidden: true },
    { type: "syncFocusTrigger" },
    { type: "manualSyncTrigger" },
  ])("recognises every sync-cadence request type ($type)", (request) => {
    expect(isSyncCadenceRequest(request)).toBe(true);
  });

  it.each<TaskWorkerRequest>([
    { type: "getFrontier" },
    { type: "runSync", nowMs: 1, trigger: "user", forceFullSweep: false, jitterUnit: 0 },
  ])("does not mistake a task request ($type) for a sync-cadence one", (request) => {
    expect(isSyncCadenceRequest(request)).toBe(false);
  });

  it.each<CalendarWorkerRequest>([{ type: "listCalendars" }, { type: "pollTimer", nowMs: 1 }])(
    "does not mistake a calendar request ($type) for a sync-cadence one",
    (request) => {
      expect(isSyncCadenceRequest(request)).toBe(false);
    },
  );

  it.each<DiagnosticsWorkerRequest>([{ type: "getDiagnostics" }, { type: "clearDiagnostics" }])(
    "does not mistake a diagnostics request ($type) for a sync-cadence one",
    (request) => {
      expect(isSyncCadenceRequest(request)).toBe(false);
    },
  );
});

describe("isDiagnosticsWorkerRequest", () => {
  it.each<DiagnosticsWorkerRequest>([{ type: "getDiagnostics" }, { type: "clearDiagnostics" }])(
    "recognises every diagnostics request type ($type)",
    (request) => {
      expect(isDiagnosticsWorkerRequest(request)).toBe(true);
    },
  );

  it.each<TaskWorkerRequest>([{ type: "getFrontier" }, { type: "getMirrorSnapshot" }])(
    "does not mistake a task request ($type) for a diagnostics one",
    (request) => {
      expect(isDiagnosticsWorkerRequest(request)).toBe(false);
    },
  );

  it.each<SyncCadenceRequest>([{ type: "syncFocusTrigger" }])(
    "does not mistake a sync-cadence request ($type) for a diagnostics one",
    (request) => {
      expect(isDiagnosticsWorkerRequest(request)).toBe(false);
    },
  );
});
