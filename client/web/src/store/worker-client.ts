import type { createCoreStore } from "./store";
import type {
  CalendarWorkerRequest,
  TaskStageName,
  TaskWorkerRequest,
  WorkerResponse,
} from "./protocol";

// The narrow slice of the DOM `MessagePort` interface a view needs — narrow
// enough that tests can pass a plain object instead of a real port. Under
// ADR-0010 (#126) the core lives in a `SharedWorker`; each view talks to it
// over `sharedWorker.port`, which this interface's shape already matches
// (it's also close enough to a dedicated `Worker`'s own message surface that
// the type carried that name through the migration).
export interface WorkerLike {
  onmessage: ((event: MessageEvent<WorkerResponse>) => void) | null;
  postMessage(message: CalendarWorkerRequest | TaskWorkerRequest): void;
}

type Store = Pick<
  ReturnType<typeof createCoreStore>,
  "setState" | "setCalendarState" | "setTaskState" | "setTaskPending"
>;

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
      case "calendarList":
        store.setCalendarState({ availableCalendars: message.calendars });
        return;
      case "currentNext":
        store.setCalendarState({
          tileKind: message.kind,
          tileEvent: message.event,
          asOfMs: message.asOfMs,
        });
        return;
      // -- task binding (#105/S7) — broadcasts fanned out to every port,
      // never a reply targeted at just the requesting view (protocol.ts).
      case "captureResult":
        store.setTaskState({
          lastCapture: {
            seed: message.seed,
            kind: message.kind,
            id: message.id,
            error: message.error,
          },
        });
        return;
      case "frontier":
        store.setTaskState({ frontier: message.items });
        return;
      case "triageInbox":
        store.setTaskState({ triageInbox: message.items });
        return;
      case "isPendingResult":
        store.setTaskPending(message.itemId, message.pending);
        return;
      case "syncOutcome":
        store.setTaskState({
          lastSyncOutcome: {
            kind: message.kind,
            retryAfterMs: message.retryAfterMs,
            activeItemCount: message.activeItemCount,
            wasFullSweep: message.wasFullSweep,
            deadLettered: message.deadLettered,
          },
        });
        return;
      case "taskEvents":
        // Same contract as the calendar binding's `credentialEvents`: at
        // least one credential-needed event landed, so reconnecting is now
        // needed. `Core::take_events` today only ever produces this one
        // kind, but the switch stays exhaustive over `TaskEventDTO["kind"]`
        // rather than assuming that never changes.
        if (message.events.some((event) => event.kind === "credential_needed")) {
          store.setTaskState({ needsReconnect: true });
        }
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

// Asks the core to re-list the picker's options. Send this only after the
// token that should be used has already been pushed: the worker processes
// requests strictly in arrival order, so a `pushToken` queued first is the
// credential this listing goes out with.
export function requestCalendarList(worker: WorkerLike): void {
  worker.postMessage({ type: "listCalendars" });
}

// -- the task binding's send helpers (#105/S7) — same "only after ready,
// never at construction time" rule as the calendar helpers above, and the
// same one-request-one-postMessage shape.

/** The host calls this at startup, once a stored device token is known, and
 * on every rotation — the key crosses this boundary exactly once per call
 * and is never read back out through any `WorkerResponse` (protocol.ts). */
export function pushTaskApiKey(worker: WorkerLike, apiKey: string): void {
  worker.postMessage({ type: "pushTaskApiKey", apiKey });
}

/** "Forget token" (#106/S8): tells the core to clear whatever credential it
 * is holding. Carries nothing and expects no reply — see `protocol.ts`'s
 * `clearTaskApiKey`. */
export function clearTaskApiKey(worker: WorkerLike): void {
  worker.postMessage({ type: "clearTaskApiKey" });
}

/** `seed` mints the deterministic id `Core::capture` derives from it; the
 * caller keeps its own seed to match the eventual `captureResult` broadcast
 * back to this specific call (see `TaskCaptureResult`, store.ts). */
export function captureTask(
  worker: WorkerLike,
  seed: string,
  title: string,
  stage: TaskStageName,
  nowMs: number,
): void {
  worker.postMessage({ type: "capture", seed, title, stage, nowMs });
}

export function requestFrontier(worker: WorkerLike): void {
  worker.postMessage({ type: "getFrontier" });
}

export function requestTriageInbox(worker: WorkerLike): void {
  worker.postMessage({ type: "getTriageInbox" });
}

export function requestIsPending(worker: WorkerLike, itemId: string): void {
  worker.postMessage({ type: "isPending", itemId });
}

export function runTaskSync(
  worker: WorkerLike,
  nowMs: number,
  trigger: "user" | "timer",
  forceFullSweep: boolean,
  jitterUnit: number,
): void {
  worker.postMessage({ type: "runSync", nowMs, trigger, forceFullSweep, jitterUnit });
}
