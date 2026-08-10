import { isInformativeSyncOutcome } from "../shell/sync-status";
import type { createCoreStore } from "./store";
import type {
  CalendarWorkerRequest,
  SyncCadenceRequest,
  TaskActionName,
  TaskStageName,
  TaskWorkerRequest,
  TriageDestinationName,
  WorkerResponse,
} from "./protocol";

/** The optional edit fields a triage mutation may carry (S13/#111) — a
 * caller-facing convenience shape over the wire message's individually
 * nullable fields (`TaskWorkerRequest`'s `"triage"` variant): an omitted key
 * here means "leave this field alone", same as an explicit `null`. */
export interface TriageEdits {
  title?: string | null;
  projectId?: string | null;
  size?: "quick" | "short" | "deep" | null;
  energy?: "low" | "medium" | "high" | null;
  context?: string | null;
}

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
  | "setState"
  | "setCalendarState"
  | "setTaskState"
  | "setTaskPending"
  | "setTaskSteps"
  | "setTaskPaneRead"
>;

// Wires a worker's response messages into the store. This is the only place
// that translates the worker protocol into store writes. Must be called in
// the same synchronous task that constructs the Worker, so the listener is
// attached before any worker message can be dispatched.
//
// Takes no clock: it used to accept an injectable `now` purely to re-request
// the context tile's current/next event after each poll outcome, and #245
// replaced that tile with the ranked pane region, whose own reads are the
// wiring hooks' business.
export function attachWorkerClient(worker: WorkerLike, store: Store): void {
  // One counter per attached worker (i.e. per view). As of issue #191 this
  // has no consumer left in view code — `useSyncWiring.ts`'s per-cycle
  // refresh, the only thing that used to key on it, was replaced by the
  // worker's own unsolicited `queueDepth`/`deadLetters` push at the tail of
  // every cycle (`worker/task-worker.ts`'s `runSync` branch). Kept anyway,
  // deliberately, not silently dropped — see `TaskState.syncOutcomeSeq`'s
  // own doc for why.
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
      case "actResult":
        store.setTaskState({
          lastAct: {
            seed: message.seed,
            itemId: message.itemId,
            action: message.action,
            kind: message.kind,
            error: message.error,
          },
        });
        if (message.kind === "ok") {
          // `Core::act`'s overlay already updated synchronously — this is
          // the same re-read `useFrontierWiring.ts` does per sync cycle,
          // triggered immediately instead of waiting for the next one, so
          // an act taken offline is visible right away (this issue's
          // "Completing offline shows Done immediately").
          requestFrontier(worker);
          requestBlocked(worker);
          // PR #207 round-2 fix: the acted-on item's `pending` must render
          // from a LIVE source. The task worker's serial queue guarantees
          // the act was applied before this reads, so `TaskState.pending`
          // gets `true` now and `false` once a sync cycle drains the queue
          // (`useItemDetailWiring` re-reads it per cycle) — which is what
          // re-enables a blocked item's Start/Cancel row.
          requestIsPending(worker, message.itemId);
        }
        return;
      case "triageResult":
        store.setTaskState({
          lastTriage: {
            seed: message.seed,
            itemId: message.itemId,
            kind: message.kind,
            error: message.error,
          },
        });
        if (message.kind === "ok") {
          // `Core::triage`'s overlay already updated synchronously — same
          // immediate re-read `actResult` triggers, so a triage taken
          // offline is visible right away (this issue's acceptance: a
          // triaged item leaves the triage query and appears on the
          // frontier through the mirror, not local bookkeeping).
          requestTriageInbox(worker);
          requestFrontier(worker);
        }
        return;
      case "setBindingResult":
        store.setTaskState({
          lastBindingWrite: {
            seed: message.seed,
            key: message.key,
            kind: message.kind,
            error: message.error,
          },
        });
        if (message.kind === "ok") {
          // `Core::set_binding`'s overlay already updated synchronously —
          // the same immediate re-read `actResult` triggers, so a binding
          // set offline is on screen without waiting for a cycle.
          requestBindings(worker);
        }
        return;
      case "bindings":
        store.setTaskState({ bindings: message.bindings });
        return;
      case "paneRead":
        // Keyed by the source the read is *for*, which the worker echoes
        // back on the message — never by which request happened to be
        // outstanding here, since this is a broadcast to every port.
        store.setTaskPaneRead(message.read.source, message.read);
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
        // The counter bumps on EVERY broadcast, whatever the kind — a cycle
        // that did not run is still a cycle that happened. As of issue #191
        // nothing in view code reads it (see this function's own doc on
        // `syncOutcomeSeq` above); it is retained rather than deleted — see
        // `TaskState.syncOutcomeSeq`'s doc for why.
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
          //
          // Issue #195 round-1 review: this used to be `now()` — this
          // view's own receipt-time clock. That is a safe stand-in for a
          // LIVE broadcast (worker posts, view receives, sub-second apart),
          // but `worker/ports.ts`'s `PortRegistry` also REPLAYS the last
          // `syncOutcome` to a port that connects long after the cycle it
          // describes; a view stamping its own clock on a replay would
          // render an hours-old cycle as "as of just now" — the exact
          // false-freshness `isInformativeSyncOutcome`/`OUTCOME_CLASS`
          // exist to prevent, two paragraphs up. `message.atMs` is the
          // cycle's OWN time (`worker/task-worker.ts` posts
          // `request.nowMs`), so it reads correctly whether this message
          // arrived live or as a replay.
          lastSyncAtMs: message.atMs,
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
        // Arrives both in reply to `getQueueDepth` (once, on becoming
        // ready) and unsolicited at the tail of every cycle (issue #191,
        // protocol.ts's `queueDepth` doc) — this handler does not need to
        // tell the two apart, since both are the same "here is the current
        // depth" fact.
        store.setTaskState({ queueDepth: message.depth });
        return;
      case "deadLetters":
        // Same dual origin as `queueDepth` above — see protocol.ts's
        // `deadLetters` doc.
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

/** The host calls this on a genuinely new or re-entered token — first-run
 * entry, or a deliberate re-submit through the 401 re-prompt — never at
 * core-start rehydration (see `initTaskApiKey` below; issue #196: only this
 * one resumes a held credential). The key crosses this boundary exactly
 * once per call and is never read back out through any `WorkerResponse`
 * (protocol.ts). */
export function pushTaskApiKey(worker: WorkerLike, apiKey: string): void {
  worker.postMessage({ type: "pushTaskApiKey", apiKey });
}

/** Issue #196 (shape 2): the host calls this — never `pushTaskApiKey` — to
 * rehydrate whatever device token is already stored: at core start, and
 * every time a view reaches `ready` under #126's one-shared-core-per-origin
 * (`useTaskTokenWiring.ts`'s core-start effect). See `protocol.ts`'s
 * `initTaskApiKey` doc for the full contract this closes — a later view
 * rehydrating an already-rejected token must not resume the hold it set. */
export function initTaskApiKey(worker: WorkerLike, apiKey: string): void {
  worker.postMessage({ type: "initTaskApiKey", apiKey });
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

/** S11/#109's act mutation: start, complete, block, cancel. `seed` mints
 * `Core::act`'s own queue-entry id — same caller-mints contract as
 * `captureTask`'s. */
export function actOnTask(
  worker: WorkerLike,
  seed: string,
  itemId: string,
  action: TaskActionName,
  nowMs: number,
): void {
  worker.postMessage({ type: "act", seed, itemId, action, nowMs });
}

/** S13/#111's triage mutation: edits whatever fields `edits` sets and
 * promotes to `destination`, as one CAS `PATCH`. `seed` mints `Core::triage`'s
 * own queue-entry id — same caller-mints contract as `actOnTask`'s. */
export function triageItem(
  worker: WorkerLike,
  seed: string,
  itemId: string,
  destination: TriageDestinationName,
  edits: TriageEdits,
  nowMs: number,
): void {
  worker.postMessage({
    type: "triage",
    seed,
    itemId,
    destination,
    title: edits.title ?? null,
    projectId: edits.projectId ?? null,
    size: edits.size ?? null,
    energy: edits.energy ?? null,
    context: edits.context ?? null,
    nowMs,
  });
}

/** #118's binding write: one absolute-value CAS `PUT`, enqueued durably.
 * `seed` mints `Core::set_binding`'s own queue-entry id — same caller-mints
 * contract as `actOnTask`'s. `key` is the kebab-case binding name; the seam
 * refuses one it cannot resolve rather than minting it. */
export function setBinding(
  worker: WorkerLike,
  seed: string,
  key: string,
  value: string,
  nowMs: number,
): void {
  worker.postMessage({ type: "setBinding", seed, key, value, nowMs });
}

/** Every standing-question binding (#118). */
export function requestBindings(worker: WorkerLike): void {
  worker.postMessage({ type: "getBindings" });
}

/** One source's pane read (#245). `nowMs` is the clock both the measured
 * ages and the alert-liveness filter are resolved against, core-side — which
 * is why it is a parameter rather than something the worker samples: the
 * caller's own tick is what makes a re-request mean anything. */
export function requestPaneRead(worker: WorkerLike, source: string, nowMs: number): void {
  worker.postMessage({ type: "getPaneRead", source, nowMs });
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

/** Issue #194: the header refresh control's task leg — sent on a manual
 * refresh press and routed through the shared cadence in `core.worker.ts`,
 * exactly like `triggerSyncFocus` above, rather than posted straight to the
 * task queue as a bespoke `runSync` (the old `runTaskSync`, deleted: it had
 * no production caller). This is what keeps a manual press ADR-0007's "same
 * cycle, user-invoked; no special path", and lets any future in-flight
 * coalescing (#184) cover it too. */
export function triggerSyncManual(worker: WorkerLike): void {
  worker.postMessage({ type: "manualSyncTrigger" });
}
