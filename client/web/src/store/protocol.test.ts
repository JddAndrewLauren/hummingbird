import { describe, expect, it } from "vitest";
import type {
  CalendarWorkerRequest,
  CaptureFieldsWire,
  TaskWorkerRequest,
  TaskWorkerResponse,
  WorkerResponse,
} from "./protocol";

/** Every capture field at its resting state. Spelled out rather than built
 * from `Object.keys`, so adding a wire field means writing it here — which is
 * the point of the wire type being nullable-required rather than optional. */
const EMPTY_CAPTURE_FIELDS: CaptureFieldsWire = {
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
};

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
    {
      type: "capture",
      seed: "seed-1",
      title: "buy milk",
      stage: "ready",
      fields: EMPTY_CAPTURE_FIELDS,
      nowMs: 1_000,
    },
    {
      type: "capture",
      seed: "seed-1",
      title: "buy milk",
      stage: "ready",
      fields: {
        ...EMPTY_CAPTURE_FIELDS,
        size: "deep",
        energy: "high",
        context: "@errands",
        description: "the oat kind",
        projectId: "proj-1",
        priority: 3,
        deadline: "2026-09-01T09:30",
        scheduledDate: "2026-08-30",
      },
      nowMs: 1_000,
    },
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
    { type: "projects", projects: [], archivedProjects: [] },
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
          entity: "items",
          entityId: "a-1",
        },
        {
          id: "item-2",
          reason: "permanent",
          message: "validation",
          fields: [],
          atMs: 6_000,
          entity: "settings",
          entityId: "theme",
        },
        // #163's third reason. `map_dead_letter` mints it in Rust and it
        // crosses a JSON boundary, so nothing but a case like this holds
        // the TS union open for it.
        {
          id: "item-3",
          reason: "contention",
          message: null,
          fields: [],
          atMs: 7_000,
          // A create whose body carried no client-minted id names no row —
          // `null` is a real answer and must survive the round trip.
          entity: "items",
          entityId: null,
        },
      ],
    },
    { type: "mirrorSnapshot", mirror: { version: 1 } },
  ])("round-trips a TaskWorkerResponse ($type)", (response) => {
    expect(roundTrip(response)).toEqual(response);
  });

  it("WorkerResponse accepts every TaskWorkerResponse variant alongside the calendar ones", () => {
    const responses: WorkerResponse[] = [
      { type: "ready", apiVersion: 1, coreId: "3f2a1b8c", viewOrdinal: 1 },
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
      "setCalendarSelections",
      "pollStart",
      "pollRefresh",
      "pollTimer",
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
