import { describe, expect, it } from "vitest";
import type { CalendarWorkerRequest, TaskWorkerRequest } from "../store/protocol";
import { isTaskWorkerRequest } from "./request-router";

describe("isTaskWorkerRequest", () => {
  it.each<TaskWorkerRequest>([
    { type: "pushTaskApiKey", apiKey: "k" },
    { type: "capture", seed: "s", title: "t", stage: "triage", nowMs: 1 },
    { type: "getFrontier" },
    { type: "getTriageInbox" },
    { type: "isPending", itemId: "i" },
    { type: "runSync", nowMs: 1, trigger: "user", forceFullSweep: false, jitterUnit: 0 },
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
});
