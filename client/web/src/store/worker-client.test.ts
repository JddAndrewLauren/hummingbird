import { describe, expect, it, vi } from "vitest";
import { type CalendarState, createCoreStore } from "./store";
import {
  attachWorkerClient,
  pollRefresh,
  pollStart,
  pollTimer,
  pushTokenToWorker,
  requestCalendarList,
  requestCurrentNext,
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
