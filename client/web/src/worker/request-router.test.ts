import { describe, expect, it } from "vitest";
import type { CalendarWorkerRequest, SyncCadenceRequest, TaskWorkerRequest } from "../store/protocol";
import { isSyncCadenceRequest, isTaskWorkerRequest } from "./request-router";

describe("isTaskWorkerRequest", () => {
  it.each<TaskWorkerRequest>([
    { type: "pushTaskApiKey", apiKey: "k" },
    { type: "initTaskApiKey", apiKey: "k" },
    { type: "clearTaskApiKey" },
    { type: "capture", seed: "s", title: "t", stage: "triage", nowMs: 1 },
    { type: "act", seed: "s", itemId: "i", action: "start", nowMs: 1 },
    {
      type: "triage",
      seed: "s",
      itemId: "i",
      destination: "ready",
      title: null,
      projectId: null,
      size: null,
      energy: null,
      context: null,
      nowMs: 1,
    },
    { type: "getFrontier" },
    { type: "getTriageInbox" },
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
    { type: "setCalendarIds", calendarIds: [] },
    { type: "pollStart", nowMs: 1 },
    { type: "pollRefresh", nowMs: 1 },
    { type: "pollTimer", nowMs: 1 },
    { type: "getCurrentNext", nowMs: 1 },
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
});
