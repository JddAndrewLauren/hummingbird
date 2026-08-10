import { isInformativeSyncOutcome } from "../shell/sync-status";
import type { createCoreStore } from "./store";
import type {
  CalendarWorkerRequest,
  SyncCadenceRequest,
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
  postMessage(message: CalendarWorkerRequest | TaskWorkerRequest | SyncCadenceRequest): void;
}

/** S9's mirror-download flow, round-1 review fix: `mirrorSnapshot` used to
 * be stored in `TaskState`, but a `WorkerResponse` is a broadcast to every
 * connected view (protocol.ts has no directed reply), so a value there
 * would retain a full copy of the mirror in EVERY open tab's memory
 * indefinitely after only one of them ever asked for a download. This is a
 * single mutable slot instead — `App.tsx` mounts exactly one
 * `useSyncWiring.ts` instance per tab, so a later registration replacing an
 * earlier one is the only case that ever happens in practice — and the
 * mirror is handed straight to whichever handler is registered right now
 * and then discarded, never retained in the store. */
let mirrorSnapshotHandler: ((mirror: unknown) => void) | null = null;

/** Registers the one handler `attachWorkerClient` hands the next
 * `mirrorSnapshot` broadcast to. Pass `null` to unregister (e.g. on
 * unmount) — a view that never asked for a download must not silently pick
 * up a snapshot some other request-issuing view triggered. */
export function setMirrorSnapshotHandler(handler: ((mirror: unknown) => void) | null): void {
  mirrorSnapshotHandler = handler;
}

type Store = Pick<
  ReturnType<typeof createCoreStore>,
  "setState" | "setCalendarState" | "setTaskState" | "setTaskPending" | "setTaskSteps"
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
  // One counter per attached worker (i.e. per view): see
  // `TaskState.syncOutcomeSeq`'s doc for why the per-cycle refresh keys on
  // this rather than on the outcome itself.
  let syncOutcomeSeq = 0;
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
      case "blocked":
        store.setTaskState({ blocked: message.entries });
        return;
      case "steps":
        store.setTaskSteps(message.itemId, message.steps);
        return;
      case "projects":
        store.setTaskState({ projects: message.projects });
        return;
      case "isPendingResult":
        store.setTaskPending(message.itemId, message.pending);
        return;
      case "syncOutcome":
        // The counter bumps on EVERY broadcast, whatever the kind — it is
        // what `useSyncWiring.ts` keys its per-cycle refresh on, and a
        // cycle that did not run is still a cycle that happened
        // (`TaskState.syncOutcomeSeq`).
        syncOutcomeSeq += 1;
        if (!isInformativeSyncOutcome(message.kind)) {
          // `"skipped"`/`"busy"`: nothing was attempted at all, so this
          // broadcast says nothing about how stale the mirror is, and
          // recording it would ERASE what does. Post-batch review found the
          // bug this closes: during a server outage the badge went red
          // ("Stale") on the first `pull_failed`, then the next 60s tick hit
          // ADR-0007's backoff, returned `Skipped`, stamped `lastSyncAtMs`
          // to now, and flipped the badge back to a green "Synced — as of
          // just now" — re-greening itself every minute for the whole
          // outage, which is precisely the affordance #107 exists to
          // provide. See `shell/sync-status.ts`'s `OUTCOME_CLASS`.
          store.setTaskState({ syncOutcomeSeq });
          return;
        }
        store.setTaskState({
          syncOutcomeSeq,
          lastSyncOutcome: {
            kind: message.kind,
            retryAfterMs: message.retryAfterMs,
            activeItemCount: message.activeItemCount,
            wasFullSweep: message.wasFullSweep,
            deadLettered: message.deadLettered,
          },
          // Every cycle that was actually ATTEMPTED counts as a "sweep" for
          // S9's status readout, whatever it resolved to — a held or failed
          // cycle is still information about how stale the mirror now is.
          lastSyncAtMs: now(),
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
      case "queueDepth":
        store.setTaskState({ queueDepth: message.depth });
        return;
      case "deadLetters":
        store.setTaskState({ deadLetters: message.entries });
        return;
      case "taskHostUnavailable":
        // Broadcast per dropped request as well as once at failure (see
        // protocol.ts), so this arm is written to be idempotent: the same
        // message landing N times is one state, not N.
        store.setTaskState({ hostError: message.message });
        return;
      case "mirrorSnapshot":
        // Never stored — see `mirrorSnapshotHandler`'s own doc.
        mirrorSnapshotHandler?.(message.mirror);
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

/** Relation-blocked items with the reason visible — S10 (issue #108). */
export function requestBlocked(worker: WorkerLike): void {
  worker.postMessage({ type: "getBlocked" });
}

/** One item's Steps — item detail (issue #96, S10). */
export function requestSteps(worker: WorkerLike, itemId: string): void {
  worker.postMessage({ type: "getSteps", itemId });
}

/** Resolves the frontier's "grouped by project" display to real names
 * (issue #108, PR #200 review). */
export function requestProjects(worker: WorkerLike): void {
  worker.postMessage({ type: "getProjects" });
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

// -- S9's sync-status reads --------------------------------------------

export function requestQueueDepth(worker: WorkerLike): void {
  worker.postMessage({ type: "getQueueDepth" });
}

export function requestDeadLetters(worker: WorkerLike): void {
  worker.postMessage({ type: "getDeadLetters" });
}

export function requestMirrorSnapshot(worker: WorkerLike): void {
  worker.postMessage({ type: "getMirrorSnapshot" });
}

// -- shared cadence coordination (S9 round-1 review, PR #181) --------------
//
// `core.worker.ts` owns ADR-0007's cadence itself now (one clock for the
// whole origin, not one per view — see that file's module doc); these two
// are the only cadence-related messages a view still sends, purely to keep
// the shared cadence honest about what it cannot observe on its own.

/** Sent on mount and on every `visibilitychange` — the worker's global
 * scope has no `document` of its own, so this is the only way it learns
 * this view's own visibility. */
export function reportViewVisibility(worker: WorkerLike, hidden: boolean): void {
  worker.postMessage({ type: "setViewVisibility", hidden });
}

/** Sent on a `window` `focus` event (ADR-0007's "on window focus"). Not
 * deduplicated across views — see `protocol.ts`'s `SyncCadenceRequest` doc
 * for why that is fine. */
export function triggerSyncFocus(worker: WorkerLike): void {
  worker.postMessage({ type: "syncFocusTrigger" });
}
