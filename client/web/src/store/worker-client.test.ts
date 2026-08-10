import { describe, expect, it, vi } from "vitest";
import { type CalendarState, type TaskState, createCoreStore } from "./store";
import {
  attachWorkerClient,
  captureTask,
  pollRefresh,
  pollStart,
  pollTimer,
  clearTaskApiKey,
  pushTaskApiKey,
  pushTokenToWorker,
  requestCalendarList,
  requestCurrentNext,
  requestDeadLetters,
  requestFrontier,
  requestIsPending,
  requestMirrorSnapshot,
  requestQueueDepth,
  requestTriageInbox,
  runTaskSync,
  setCalendarIdsOnWorker,
  type WorkerLike,
} from "./worker-client";

const initialCalendar: CalendarState = {
  connected: false,
  needsReconnect: false,
  selectedCalendarIds: [],
  availableCalendars: [],
  lastPollOutcome: null,
  tileKind: "no_snapshot",
  tileEvent: null,
  asOfMs: null,
};

const initialTask: TaskState = {
  frontier: [],
  triageInbox: [],
  pending: {},
  lastCapture: null,
  lastSyncOutcome: null,
  lastSyncAtMs: null,
  queueDepth: null,
  deadLetters: [],
  mirrorSnapshot: null,
  needsReconnect: false,
};

// A minimal fake of the Worker surface the client needs: an assignable
// onmessage handler the test can drive directly, plus a spyable
// postMessage. The *response* (attachWorkerClient) side never calls
// postMessage itself — only the explicit send helpers below do, and only
// ever after the store has already observed "ready" (PR #79 round-2
// blocker — see protocol.ts).
function fakeWorker(): WorkerLike & { postMessage: ReturnType<typeof vi.fn> } {
  return {
    onmessage: null,
    postMessage: vi.fn(),
  };
}

describe("attachWorkerClient", () => {
  it("only listens on attach — sends nothing, so there is no init race to lose", () => {
    const worker = fakeWorker();
    const store = createCoreStore();

    attachWorkerClient(worker, store);

    expect(worker.onmessage).toBeTypeOf("function");
    expect(store.getSnapshot().status).toBe("loading");
    expect(worker.postMessage).not.toHaveBeenCalled();
  });

  it("moves the store to ready with the reported api version on a ready message", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({ data: { type: "ready", apiVersion: 3 } } as MessageEvent);

    expect(store.getSnapshot()).toEqual({
      status: "ready",
      apiVersion: 3,
      error: null,
      calendar: initialCalendar,
      task: initialTask,
    });
  });

  it("moves the store to error with the reported message on an error message", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: { type: "error", message: "wasm init failed" },
    } as MessageEvent);

    expect(store.getSnapshot()).toEqual({
      status: "error",
      apiVersion: null,
      error: "wasm init failed",
      calendar: initialCalendar,
      task: initialTask,
    });
  });

  it("records the latest poll outcome and re-requests currentNext on a pollOutcome message", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store, () => 7_000);

    worker.onmessage?.({
      data: { type: "pollOutcome", outcome: "succeeded" },
    } as MessageEvent);

    expect(store.getSnapshot().calendar.lastPollOutcome).toBe("succeeded");
    expect(worker.postMessage).toHaveBeenCalledWith({
      type: "getCurrentNext",
      nowMs: 7_000,
    });
  });

  it("flags needsReconnect when a non-empty credentialEvents message arrives", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: {
        type: "credentialEvents",
        events: [{ provider: "google_calendar", atMs: 1000 }],
      },
    } as MessageEvent);

    expect(store.getSnapshot().calendar.needsReconnect).toBe(true);
  });

  it("does not flag needsReconnect on an empty credentialEvents message", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: { type: "credentialEvents", events: [] },
    } as MessageEvent);

    expect(store.getSnapshot().calendar.needsReconnect).toBe(false);
  });

  it("writes the tile fields on a currentNext message", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: {
        type: "currentNext",
        kind: "upcoming",
        event: {
          title: "Standup",
          startMs: 1_000,
          endMs: 2_000,
          allDay: false,
          htmlLink: null,
        },
        asOfMs: 500,
      },
    } as MessageEvent);

    expect(store.getSnapshot().calendar).toEqual({
      ...initialCalendar,
      tileKind: "upcoming",
      tileEvent: {
        title: "Standup",
        startMs: 1_000,
        endMs: 2_000,
        allDay: false,
        htmlLink: null,
      },
      asOfMs: 500,
    });
  });

  it("writes the picker's options on a calendarList message", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: {
        type: "calendarList",
        calendars: [{ id: "primary", summary: "john@twinion.net" }],
      },
    } as MessageEvent);

    expect(store.getSnapshot().calendar).toEqual({
      ...initialCalendar,
      availableCalendars: [{ id: "primary", summary: "john@twinion.net" }],
    });
  });

  // -- task binding (#105/S7) -----------------------------------------

  it("records a captureResult keyed by its seed", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: { type: "captureResult", seed: "seed-1", kind: "ok", id: "item-1", error: null },
    } as MessageEvent);

    expect(store.getSnapshot().task.lastCapture).toEqual({
      seed: "seed-1",
      kind: "ok",
      id: "item-1",
      error: null,
    });
  });

  it("writes the frontier on a frontier message", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    const item = {
      id: "item-1",
      seq: null,
      title: "buy milk",
      description: null,
      stage: "ready" as const,
      size: null,
      energy: null,
      context: null,
      priority: 0,
      projectId: null,
      projectPos: null,
      dueDate: null,
      scheduledDate: null,
      source: null,
      sourceKey: null,
      sourceUrl: null,
      archivedAt: null,
      createdAt: 1,
      updatedAt: 1,
      version: 0,
    };
    worker.onmessage?.({ data: { type: "frontier", items: [item] } } as MessageEvent);

    expect(store.getSnapshot().task.frontier).toEqual([item]);
    // Untouched sibling field.
    expect(store.getSnapshot().task.triageInbox).toEqual([]);
  });

  it("writes the triage inbox on a triageInbox message", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({ data: { type: "triageInbox", items: [] } } as MessageEvent);

    expect(store.getSnapshot().task.triageInbox).toEqual([]);
  });

  it("merges one item's pending state on an isPendingResult message, leaving others alone", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: { type: "isPendingResult", itemId: "item-1", pending: true },
    } as MessageEvent);
    worker.onmessage?.({
      data: { type: "isPendingResult", itemId: "item-2", pending: false },
    } as MessageEvent);

    expect(store.getSnapshot().task.pending).toEqual({ "item-1": true, "item-2": false });
  });

  it("records the sync outcome and the sweep time on a syncOutcome message", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store, () => 5_000);

    worker.onmessage?.({
      data: {
        type: "syncOutcome",
        kind: "completed",
        retryAfterMs: null,
        activeItemCount: 2,
        wasFullSweep: true,
        deadLettered: 0,
      },
    } as MessageEvent);

    expect(store.getSnapshot().task.lastSyncOutcome).toEqual({
      kind: "completed",
      retryAfterMs: null,
      activeItemCount: 2,
      wasFullSweep: true,
      deadLettered: 0,
    });
    expect(store.getSnapshot().task.lastSyncAtMs).toBe(5_000);
  });

  it("records the sweep time on a held or failed outcome too — staleness must not freeze", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store, () => 7_000);

    worker.onmessage?.({
      data: {
        type: "syncOutcome",
        kind: "held",
        retryAfterMs: null,
        activeItemCount: null,
        wasFullSweep: null,
        deadLettered: null,
      },
    } as MessageEvent);

    expect(store.getSnapshot().task.lastSyncAtMs).toBe(7_000);
  });

  it("records the queue depth on a queueDepth message", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({ data: { type: "queueDepth", depth: 3 } } as MessageEvent);

    expect(store.getSnapshot().task.queueDepth).toBe(3);
  });

  it("records the dead-letter journal on a deadLetters message", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);
    const entries = [
      {
        id: "item-1",
        reason: "conflict" as const,
        message: null,
        fields: [{ field: "title", local: "buy oat milk", server: "someone else's" }],
        atMs: 1_000,
      },
    ];

    worker.onmessage?.({ data: { type: "deadLetters", entries } } as MessageEvent);

    expect(store.getSnapshot().task.deadLetters).toEqual(entries);
  });

  it("records the mirror snapshot on a mirrorSnapshot message", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: { type: "mirrorSnapshot", mirror: { version: 1 } },
    } as MessageEvent);

    expect(store.getSnapshot().task.mirrorSnapshot).toEqual({ version: 1 });
  });

  it("flags task needsReconnect on a taskEvents message carrying a credential_needed event", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: { type: "taskEvents", events: [{ kind: "credential_needed", atMs: 1_000 }] },
    } as MessageEvent);

    expect(store.getSnapshot().task.needsReconnect).toBe(true);
  });

  it("does not flag task needsReconnect on an empty taskEvents message", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({ data: { type: "taskEvents", events: [] } } as MessageEvent);

    expect(store.getSnapshot().task.needsReconnect).toBe(false);
  });
});

describe("the calendar send helpers", () => {
  it("pushTokenToWorker posts a pushToken request", () => {
    const worker = fakeWorker();
    pushTokenToWorker(worker, "tok-1");
    expect(worker.postMessage).toHaveBeenCalledWith({
      type: "pushToken",
      token: "tok-1",
    });
  });

  it("setCalendarIdsOnWorker posts a setCalendarIds request", () => {
    const worker = fakeWorker();
    setCalendarIdsOnWorker(worker, ["a", "b"]);
    expect(worker.postMessage).toHaveBeenCalledWith({
      type: "setCalendarIds",
      calendarIds: ["a", "b"],
    });
  });

  it("pollStart/pollRefresh/pollTimer/requestCurrentNext post their matching request", () => {
    const worker = fakeWorker();

    pollStart(worker, 1);
    pollRefresh(worker, 2);
    pollTimer(worker, 3);
    requestCurrentNext(worker, 4);

    expect(worker.postMessage).toHaveBeenNthCalledWith(1, {
      type: "pollStart",
      nowMs: 1,
    });
    expect(worker.postMessage).toHaveBeenNthCalledWith(2, {
      type: "pollRefresh",
      nowMs: 2,
    });
    expect(worker.postMessage).toHaveBeenNthCalledWith(3, {
      type: "pollTimer",
      nowMs: 3,
    });
    expect(worker.postMessage).toHaveBeenNthCalledWith(4, {
      type: "getCurrentNext",
      nowMs: 4,
    });
  });

  it("requestCalendarList posts a listCalendars request with no token", () => {
    // The core lists with the credential it was already pushed (ADR-0003:
    // the core owns HTTP), so this side never handles a token for it.
    const worker = fakeWorker();
    requestCalendarList(worker);
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "listCalendars" });
  });
});

describe("the task send helpers (#105/S7)", () => {
  it("pushTaskApiKey posts a pushTaskApiKey request", () => {
    const worker = fakeWorker();
    pushTaskApiKey(worker, "device-token-1");
    expect(worker.postMessage).toHaveBeenCalledWith({
      type: "pushTaskApiKey",
      apiKey: "device-token-1",
    });
  });

  it("clearTaskApiKey posts a clearTaskApiKey request carrying nothing", () => {
    const worker = fakeWorker();
    clearTaskApiKey(worker);
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "clearTaskApiKey" });
  });

  it("captureTask posts a capture request carrying its seed", () => {
    const worker = fakeWorker();
    captureTask(worker, "seed-1", "buy milk", "ready", 1_000);
    expect(worker.postMessage).toHaveBeenCalledWith({
      type: "capture",
      seed: "seed-1",
      title: "buy milk",
      stage: "ready",
      nowMs: 1_000,
    });
  });

  it("requestFrontier/requestTriageInbox/requestIsPending post their matching request", () => {
    const worker = fakeWorker();

    requestFrontier(worker);
    requestTriageInbox(worker);
    requestIsPending(worker, "item-1");

    expect(worker.postMessage).toHaveBeenNthCalledWith(1, { type: "getFrontier" });
    expect(worker.postMessage).toHaveBeenNthCalledWith(2, { type: "getTriageInbox" });
    expect(worker.postMessage).toHaveBeenNthCalledWith(3, {
      type: "isPending",
      itemId: "item-1",
    });
  });

  it("runTaskSync posts a runSync request", () => {
    const worker = fakeWorker();
    runTaskSync(worker, 1_000, "timer", false, 0.5);
    expect(worker.postMessage).toHaveBeenCalledWith({
      type: "runSync",
      nowMs: 1_000,
      trigger: "timer",
      forceFullSweep: false,
      jitterUnit: 0.5,
    });
  });

  it("requestQueueDepth posts a getQueueDepth request", () => {
    const worker = fakeWorker();
    requestQueueDepth(worker);
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "getQueueDepth" });
  });

  it("requestDeadLetters posts a getDeadLetters request", () => {
    const worker = fakeWorker();
    requestDeadLetters(worker);
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "getDeadLetters" });
  });

  it("requestMirrorSnapshot posts a getMirrorSnapshot request", () => {
    const worker = fakeWorker();
    requestMirrorSnapshot(worker);
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "getMirrorSnapshot" });
  });
});
