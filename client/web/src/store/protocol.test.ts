import { describe, expect, it } from "vitest";
import type {
  CalendarWorkerRequest,
  TaskWorkerRequest,
  TaskWorkerResponse,
  WorkerResponse,
} from "./protocol";

// The wire is `structuredClone`-compatible `postMessage` traffic in
// production; `JSON.parse(JSON.stringify(...))` is the same round-trip for
// every value these types actually carry (strings, numbers, booleans,
// null, arrays, plain objects — nothing these unions carry needs anything
// `structuredClone` can do that `JSON` cannot).
function roundTrip<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

describe("protocol round-trips", () => {
  it.each<TaskWorkerRequest>([
    { type: "pushTaskApiKey", apiKey: "device-token-1" },
    { type: "initTaskApiKey", apiKey: "device-token-1" },
    { type: "clearTaskApiKey" },
    { type: "capture", seed: "seed-1", title: "buy milk", stage: "ready", nowMs: 1_000 },
    { type: "getFrontier" },
    { type: "getTriageInbox" },
    { type: "getBlocked" },
    { type: "getSteps", itemId: "item-1" },
    { type: "getProjects" },
    { type: "isPending", itemId: "item-1" },
    { type: "runSync", nowMs: 1_000, trigger: "timer", forceFullSweep: true, jitterUnit: 0.5 },
    { type: "getQueueDepth" },
    { type: "getDeadLetters" },
    { type: "getMirrorSnapshot" },
  ])("round-trips a TaskWorkerRequest ($type)", (request) => {
    expect(roundTrip(request)).toEqual(request);
  });

  it.each<TaskWorkerResponse>([
    { type: "captureResult", seed: "seed-1", kind: "ok", id: "item-1", error: null },
    { type: "captureResult", seed: "seed-1", kind: "failed", id: null, error: "boom" },
    { type: "frontier", items: [] },
    { type: "triageInbox", items: [] },
    { type: "blocked", entries: [] },
    { type: "steps", itemId: "item-1", steps: [] },
    { type: "projects", projects: [] },
    { type: "isPendingResult", itemId: "item-1", pending: true },
    {
      type: "syncOutcome",
      kind: "completed",
      retryAfterMs: null,
      activeItemCount: 3,
      wasFullSweep: false,
      deadLettered: 0,
      atMs: 5_000,
    },
    { type: "taskEvents", events: [{ kind: "credential_needed", atMs: 5_000 }] },
    { type: "queueDepth", depth: 3 },
    {
      type: "deadLetters",
      entries: [
        {
          id: "item-1",
          reason: "conflict",
          message: null,
          fields: [{ field: "title", local: "buy oat milk", server: "someone else's" }],
          atMs: 5_000,
        },
        {
          id: "item-2",
          reason: "permanent",
          message: "validation",
          fields: [],
          atMs: 6_000,
        },
      ],
    },
    { type: "mirrorSnapshot", mirror: { version: 1 } },
  ])("round-trips a TaskWorkerResponse ($type)", (response) => {
    expect(roundTrip(response)).toEqual(response);
  });

  it("WorkerResponse accepts every TaskWorkerResponse variant alongside the calendar ones", () => {
    const responses: WorkerResponse[] = [
      { type: "ready", apiVersion: 1 },
      { type: "error", message: "boom" },
      { type: "pollOutcome", outcome: "succeeded" },
      { type: "frontier", items: [] },
      { type: "taskEvents", events: [] },
    ];

    for (const response of responses) {
      expect(roundTrip(response)).toEqual(response);
    }
  });

  it("no CalendarWorkerRequest and TaskWorkerRequest type name collides", () => {
    const calendarTypes: CalendarWorkerRequest["type"][] = [
      "pushToken",
      "setCalendarIds",
      "pollStart",
      "pollRefresh",
      "pollTimer",
      "getCurrentNext",
      "listCalendars",
    ];
    const taskTypes: TaskWorkerRequest["type"][] = [
      "pushTaskApiKey",
      "initTaskApiKey",
      "clearTaskApiKey",
      "capture",
      "getFrontier",
      "getTriageInbox",
      "getBlocked",
      "getSteps",
      "getProjects",
      "isPending",
      "runSync",
      "getQueueDepth",
      "getDeadLetters",
      "getMirrorSnapshot",
    ];

    const overlap = calendarTypes.filter((type) => (taskTypes as string[]).includes(type));
    expect(overlap).toEqual([]);
  });
});
