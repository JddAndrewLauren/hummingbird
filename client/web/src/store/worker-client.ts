import type { createCoreStore } from "./store";
import type { CalendarWorkerRequest, WorkerResponse } from "./protocol";

// The narrow slice of the DOM Worker interface the client needs — narrow
// enough that tests can pass a plain object instead of a real Worker.
export interface WorkerLike {
  onmessage: ((event: MessageEvent<WorkerResponse>) => void) | null;
  postMessage(message: CalendarWorkerRequest): void;
}

type Store = Pick<ReturnType<typeof createCoreStore>, "setState" | "setCalendarState">;

// Wires a worker's response messages into the store. This is the only place
// that translates the worker protocol into store writes. Must be called in
// the same synchronous task that constructs the Worker, so the listener is
// attached before any worker message can be dispatched.
//
// `now` defaults to `Date.now` and is only overridden in tests: after every
// poll outcome, the client also asks the worker for the fresh current/next
// event (issue #73's tile), so the tile never trails a poll it already knows
// completed.
export function attachWorkerClient(
  worker: WorkerLike,
  store: Store,
  now: () => number = Date.now,
): void {
  worker.onmessage = (event) => {
    const message = event.data;
    switch (message.type) {
      case "ready":
        store.setState({
          status: "ready",
          apiVersion: message.apiVersion,
          error: null,
        });
        return;
      case "error":
        store.setState({ status: "error", error: message.message });
        return;
      case "pollOutcome":
        store.setCalendarState({ lastPollOutcome: message.outcome });
        requestCurrentNext(worker, now());
        return;
      case "credentialEvents":
        // At least one credential-needed event landed: the calendar app
        // layer (calendar/connection.ts) is what answers it with a silent
        // re-mint or a re-connect affordance — the store just records that
        // reconnecting is now needed so the UI can offer it.
        if (message.events.length > 0) {
          store.setCalendarState({ needsReconnect: true });
        }
        return;
      case "currentNext":
        store.setCalendarState({
          tileKind: message.kind,
          tileEvent: message.event,
          asOfMs: message.asOfMs,
        });
        return;
    }
  };
}

// The main thread only ever calls these AFTER observing "ready" — sending a
// request at Worker construction time races the worker's async module
// evaluation and is silently dropped (PR #79 round-2 blocker; see
// protocol.ts). Each is a thin `postMessage` wrapper so call sites read as
// intent rather than hand-built message objects.

export function pushTokenToWorker(worker: WorkerLike, token: string): void {
  worker.postMessage({ type: "pushToken", token });
}

export function setCalendarIdsOnWorker(worker: WorkerLike, calendarIds: string[]): void {
  worker.postMessage({ type: "setCalendarIds", calendarIds });
}

export function pollStart(worker: WorkerLike, nowMs: number): void {
  worker.postMessage({ type: "pollStart", nowMs });
}

export function pollRefresh(worker: WorkerLike, nowMs: number): void {
  worker.postMessage({ type: "pollRefresh", nowMs });
}

export function pollTimer(worker: WorkerLike, nowMs: number): void {
  worker.postMessage({ type: "pollTimer", nowMs });
}

export function requestCurrentNext(worker: WorkerLike, nowMs: number): void {
  worker.postMessage({ type: "getCurrentNext", nowMs });
}
