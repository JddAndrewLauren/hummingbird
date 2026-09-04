import { isInformativeSyncOutcome } from "../shell/sync-outcome-informative";
import {
  advanceLastSuccessfulSyncAtMs,
  readLastSuccessfulSyncAtMs,
  type SyncStorageLike,
} from "./sync-persistence";
import type { createCoreStore } from "./store";
import type {
  CalendarSelectionDTO,
  CalendarWorkerRequest,
  ConditionDTO,
  DiagnosticEventV1DTO,
  DiagnosticsWorkerRequest,
  GrillDraftTurnDTO,
  GrillVerdictName,
  ProjectDTO,
  ProjectLinkDTO,
  RouteDTO,
  RuleDTO,
  StepDTO,
  SyncCadenceRequest,
  TaskActionName,
  TaskStageName,
  TaskWorkerRequest,
  TierName,
  TriageEdits,
  WorkerResponse,
} from "./protocol";

/** Re-exported so a screen importing the triage mutation gets its edit shape
 * from the same module (the type itself lives with the rest of the wire
 * contract, `store/protocol.ts` — an absent key leaves a field alone, a `null`
 * clears it, a value sets it; `scheduledDate` — #122's do-date edit — is the
 * same absent/null/value contract as every other field, not a separate
 * clear-flag shape). */
export type { TriageEdits } from "./protocol";

/** The capture box's optional field selections (#208) — the caller-facing
 * convenience shape over `TaskWorkerRequest`'s `"capture"` variant, same
 * "omitted means unset" contract `TriageEdits` documents for its own fields.
 * `screens/capture-meta.ts`'s `resolveCaptureFields` is what turns the capture
 * box's live controls into this shape.
 *
 * Optional here and required-but-nullable on the wire (`CaptureFieldsWire`):
 * a caller sets what it has, and `captureTask` below fills in the `null`s, so
 * a field added to the wire cannot be silently forgotten by this side. */
export interface CaptureFields {
  size?: "quick" | "normal" | "deep" | null;
  energy?: "low" | "medium" | "high" | null;
  context?: string | null;
  description?: string | null;
  projectId?: string | null;
  priority?: number | null;
  deadline?: string | null;
  scheduledDate?: string | null;
  linkUrl?: string | null;
  linkLabel?: string | null;
}

// The narrow slice of the DOM `MessagePort` interface a view needs — narrow
// enough that tests can pass a plain object instead of a real port. Under
// ADR-0010 (#126) the core lives in a `SharedWorker`; each view talks to it
// over `sharedWorker.port`, which this interface's shape already matches
// (it's also close enough to a dedicated `Worker`'s own message surface that
// the type carried that name through the migration).
export interface WorkerLike {
  onmessage: ((event: MessageEvent<WorkerResponse>) => void) | null;
  postMessage(
    message: CalendarWorkerRequest | TaskWorkerRequest | SyncCadenceRequest | DiagnosticsWorkerRequest,
  ): void;
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

/** #707's "Download diagnostics" flow — the identical single-mutable-slot
 * shape `mirrorSnapshotHandler` documents above, for the same reason: a
 * `diagnosticsExport` is a broadcast with no directed reply, so storing it
 * in `TaskState` would retain a full journal export in every open tab's
 * memory indefinitely after only one of them ever asked for a download. */
let diagnosticsExportHandler:
  | ((events: DiagnosticEventV1DTO[], droppedCount: number) => void)
  | null = null;

/** Registers the one handler `attachWorkerClient` hands the next
 * `diagnosticsExport` broadcast to. Pass `null` to unregister (e.g. on
 * unmount) — the same contract `setMirrorSnapshotHandler` documents. */
export function setDiagnosticsExportHandler(
  handler: ((events: DiagnosticEventV1DTO[], droppedCount: number) => void) | null,
): void {
  diagnosticsExportHandler = handler;
}

type Store = Pick<
  ReturnType<typeof createCoreStore>,
  | "setState"
  | "setCalendarState"
  | "setCalendarEventRead"
  | "setTaskState"
  | "setTaskPending"
  | "setTaskSteps"
  | "setTaskProjectLinks"
  | "setTaskRoute"
  | "setTaskPaneRead"
  | "setTaskGrillDraft"
>;

// Wires a worker's response messages into the store. This is the only place
// that translates the worker protocol into store writes. Must be called in
// the same synchronous task that constructs the Worker, so the listener is
// attached before any worker message can be dispatched.
//
// Outcome timestamps always come from the worker message, never this view's
// receipt clock: replay must retain its real age. A completed outcome samples
// `Date.now()` only to detect a persisted timestamp that is now in the future
// after a wall-clock correction; it never stamps an outcome or owns a timer.
export function attachWorkerClient(
  worker: WorkerLike,
  store: Store,
  storage?: SyncStorageLike,
): void {
  // One counter per attached worker (i.e. per view). As of issue #191 this
  // has no consumer left in view code — `useSyncWiring.ts`'s per-cycle
  // refresh, the only thing that used to key on it, was replaced by the
  // worker's own unsolicited `queueDepth`/`deadLetters` push at the tail of
  // every cycle (`worker/task-worker.ts`'s `runSync` branch). Kept anyway,
  // deliberately, not silently dropped — see `TaskState.syncOutcomeSeq`'s
  // own doc for why.
  let syncOutcomeSeq = 0;
  let lastSuccessfulSyncAtMs = storage ? readLastSuccessfulSyncAtMs(storage) : null;
  store.setTaskState({ lastSuccessfulSyncAtMs });
  worker.onmessage = (event) => {
    const message = event.data;
    switch (message.type) {
      case "ready":
        store.setState({
          status: "ready",
          apiVersion: message.apiVersion,
          coreId: message.coreId,
          viewOrdinal: message.viewOrdinal,
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
      case "calendarEvents":
        // Keyed by the request's own `key`, echoed back on the message —
        // never by which request happened to be outstanding here, since
        // this is a broadcast to every port (same discipline `paneRead`
        // documents for `source`).
        store.setCalendarEventRead(message.key, message.read);
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
          // `ItemAction::Block` is the one act that moves an item INTO the
          // external-wait list, and Start/Complete/Cancel on an already
          // blocked item move it out — either way the standing questions'
          // inputs are stale until this re-reads (#675).
          requestExternallyBlocked(worker);
          // The row checkmark completes from ANY live stage — Triage rows
          // included — so an act can now remove an item from the triage
          // inbox, the same immediate re-read `triageResult` does.
          requestTriageInbox(worker);
          // A Grilling item is just as actable (`canMarkDone`/`item-actions.ts`
          // gates on stage, not this read) — same immediate re-read for the
          // same reason.
          requestGrillingItems(worker);
          // The Ledger/Done refresh an act also warrants is NOT here:
          // `getLedger` carries a `nowMs` this module deliberately never
          // samples (see this function's doc), so `useLedgerWiring.ts` keys
          // on `lastAct` and re-reads both with its own clock.
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
          requestGrillingItems(worker);
          requestFrontier(worker);
          // #122: a `null`-destination triage (the weekend-plans pane's
          // do-date chip) can touch an item sitting in `task.blocked` —
          // relation-blocked but still Ready/InProgress, exactly
          // `actResult`'s own targets above — so that list needs the same
          // immediate re-read or a just-set do-date reads stale there until
          // the next sync cycle.
          requestBlocked(worker);
        }
        return;
      case "completeGrillResult":
        store.setTaskState({
          lastGrillCompletion: {
            seed: message.seed,
            itemId: message.itemId,
            kind: message.kind,
            grillId: message.grillId,
            error: message.error,
          },
        });
        if (message.kind === "ok") {
          // `Core::complete_grill`'s overlay already updated the item's
          // stage synchronously — same immediate re-read `triageResult`
          // triggers, so a confirmed Grill is visible right away, offline
          // or not.
          requestTriageInbox(worker);
          // A `fog_remains` verdict demotes the item straight into Grilling
          // (`task_host.rs`'s `complete_grill` doc) — same immediate
          // re-read as `triageInbox` above.
          requestGrillingItems(worker);
          requestFrontier(worker);
          requestBlocked(worker);
          // A ticked `deleteUntickedPlan` soft-deletes the item's live
          // Steps core-side — the checklist this view already read for
          // that item (if any) needs the same immediate re-read, or the
          // now-deleted rows would sit stale until the next sync cycle.
          requestSteps(worker, message.itemId);
        }
        return;
      // Save and discard share one result slot — a caller only ever cares
      // "did my last draft write land", not which kind of write it was.
      case "saveGrillDraftResult":
      case "discardGrillDraftResult":
        store.setTaskState({
          lastGrillDraftWrite: {
            itemId: message.itemId,
            kind: message.kind,
            error: message.error,
          },
        });
        return;
      case "grillDraft":
        // Every real answer installs an entry — `stepsByItem`'s own "only
        // grows entries actually asked about" shape, but unlike that read
        // `exists: false` is NOT skipped here: `useGrillTakeoverWiring.ts`'s
        // resume wait has no other way to learn the wait is over, and a
        // race (the bulk `grillDraftItemIds` list said yes, a concurrent
        // discard in another tab made this per-item read say no by the
        // time it landed) must resolve as "resume with nothing" rather than
        // leave that session's interview stuck at `idle` forever.
        store.setTaskGrillDraft(message.itemId, message.exists ? message.turns ?? [] : []);
        return;
      case "grillDraftItemIds":
        store.setTaskState({ grillDraftItemIds: message.itemIds });
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
      case "setQuestionEnabledResult":
        store.setTaskState({
          lastQuestionSwitchWrite: {
            seed: message.seed,
            question: message.question,
            kind: message.kind,
            error: message.error,
          },
        });
        if (message.kind === "ok") {
          // `Core::set_question_enabled` overlays, exactly as
          // `set_binding` does, so the toggle settles into its new state
          // without waiting for a cycle.
          requestQuestionSwitches(worker);
        }
        return;
      case "questionSwitches":
        store.setTaskState({ questionSwitches: message.switches });
        return;
      case "kindRegistry":
        store.setTaskState({ kindRegistry: message.registry });
        return;
      case "rules":
        store.setTaskState({ rules: message.rules });
        return;
      case "createRuleResult":
        store.setTaskState({
          lastRuleWrite: { seed: message.seed, ruleId: message.id, kind: message.kind, error: message.error },
        });
        if (message.kind === "ok") {
          requestRules(worker);
        }
        return;
      case "patchRuleResult":
        store.setTaskState({
          lastRuleWrite: { seed: message.seed, ruleId: message.ruleId, kind: message.kind, error: message.error },
        });
        if (message.kind === "ok") {
          // No overlay for rules (`Core::rules`'s own doc) — the change
          // becomes visible once the next completed cycle pulls it back,
          // so this re-request is the same best-effort read every other
          // mutation result triggers, not an immediate-visibility guarantee.
          requestRules(worker);
        }
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
      case "grillingItems":
        store.setTaskState({ grillingItems: message.items });
        return;
      case "externallyBlocked":
        store.setTaskState({ externallyBlocked: message.items });
        return;
      case "ledger":
        store.setTaskState({ ledger: message.rows });
        return;
      case "searchResult":
        store.setTaskState({ search: { rows: message.rows, total: message.total } });
        return;
      case "done":
        store.setTaskState({ done: message.items });
        return;
      case "blocked":
        store.setTaskState({ blocked: message.entries });
        return;
      case "steps":
        store.setTaskSteps(message.itemId, message.steps);
        return;
      case "projects":
        store.setTaskState({
          projects: message.projects,
          archivedProjects: message.archivedProjects,
        });
        return;
      case "createProjectResult":
        store.setTaskState({
          lastProjectWrite: {
            seed: message.seed,
            projectId: message.id,
            kind: message.kind,
            error: message.error,
          },
        });
        if (message.kind === "ok") {
          // No overlay for projects (`Core::create_project`'s own doc) — the
          // new project becomes visible once the next completed cycle pulls
          // it back, so this re-request answers the *old* list, which is the
          // point: the grid says it is waiting rather than showing a card
          // the authority has not confirmed.
          requestProjects(worker);
        }
        return;
      case "patchProjectResult":
        store.setTaskState({
          lastProjectWrite: {
            seed: message.seed,
            projectId: message.projectId,
            kind: message.kind,
            error: message.error,
          },
        });
        if (message.kind === "ok") {
          // No overlay for projects — same reasoning as `createProjectResult`:
          // the edit becomes visible once the next completed cycle pulls it
          // back, so this re-request answers the *old* row until then.
          requestProjects(worker);
        }
        return;
      case "projectLinks":
        store.setTaskProjectLinks(message.projectId, message.links);
        return;
      case "createProjectLinkResult":
        store.setTaskState({
          lastProjectLinkWrite: {
            seed: message.seed,
            projectId: message.projectId,
            kind: message.kind,
            error: message.error,
          },
        });
        if (message.kind === "ok") {
          // No overlay for links (`Core::create_project_link`'s own doc) —
          // the new link becomes visible once the next completed cycle
          // pulls it back, so this re-request answers the *old* list,
          // which is the point: the card says it is waiting rather than
          // showing a row the authority has not confirmed.
          requestProjectLinks(worker, message.projectId);
        }
        return;
      case "patchProjectLinkResult":
        store.setTaskState({
          lastProjectLinkWrite: {
            seed: message.seed,
            projectId: message.projectId,
            kind: message.kind,
            error: message.error,
          },
        });
        if (message.kind === "ok") {
          // No overlay for links, same reasoning as `patchProjectResult`:
          // the edit becomes visible once the next completed cycle pulls
          // it back, so this re-request answers the *old* row until then.
          requestProjectLinks(worker, message.projectId);
        }
        return;
      case "route":
        store.setTaskRoute(message.projectId, message.route);
        return;
      case "patchRouteResult":
        store.setTaskState({
          lastRouteWrite: {
            seed: message.seed,
            projectId: message.projectId,
            kind: message.kind,
            error: message.error,
          },
        });
        if (message.kind === "ok") {
          // No overlay for routes (`Core::patch_route`'s own doc) — the
          // edit becomes visible once the next completed cycle pulls it
          // back, so this re-request answers the *old* row until then, and
          // is also how the dossier discovers a 409 that landed on it: the
          // row's `version` simply does not move to what this write
          // expected.
          requestRoute(worker, message.projectId);
        }
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
        if (message.kind === "completed" && message.atMs >= 0 && Number.isFinite(message.atMs)) {
          const advanced = storage
            ? advanceLastSuccessfulSyncAtMs(
                storage,
                lastSuccessfulSyncAtMs,
                message.atMs,
                Date.now(),
              )
            : lastSuccessfulSyncAtMs === null || message.atMs > lastSuccessfulSyncAtMs
              ? message.atMs
              : lastSuccessfulSyncAtMs;
          if (advanced !== lastSuccessfulSyncAtMs) {
            lastSuccessfulSyncAtMs = advanced;
            store.setTaskState({ lastSuccessfulSyncAtMs });
          }
        }
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
          // provide. See `shell/sync-outcome-informative.ts`'s
          // `isInformativeSyncOutcome` (#535 moved the classification here
          // from `shell/sync-status.ts`'s `OUTCOME_CLASS`).
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
      case "diagnosticsExport":
        // Never stored — see `diagnosticsExportHandler`'s own doc.
        diagnosticsExportHandler?.(message.events, message.droppedCount);
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

export function setCalendarSelectionsOnWorker(
  worker: WorkerLike,
  selections: CalendarSelectionDTO[],
): void {
  worker.postMessage({ type: "setCalendarSelections", selections });
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

/** Issue #267: the non-cancelled events overlapping `[startMs, endMs)`.
 * `key` is caller-chosen and comes back unchanged on the `calendarEvents`
 * broadcast — see `protocol.ts`'s `getCalendarEvents` doc for why the
 * calendar lane keys by request rather than by source. */
export function requestCalendarEvents(
  worker: WorkerLike,
  key: string,
  startMs: number,
  endMs: number,
  startDate: string,
  endDate: string,
  nowMs: number,
): void {
  worker.postMessage({
    type: "getCalendarEvents",
    key,
    startMs,
    endMs,
    startDate,
    endDate,
    nowMs,
  });
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
 * back to this specific call (see `TaskCaptureResult`, store.ts).
 *
 * `fields` (#208) carries the capture box's Energy/Size/Context selections
 * onto the same wire message — an omitted key or an explicit `null` both
 * mean "not set", so the "leaving all three at rest still absent" contract
 * survives whether a caller passes `{}` or nothing at all. */
export function captureTask(
  worker: WorkerLike,
  seed: string,
  title: string,
  stage: TaskStageName,
  nowMs: number,
  fields: CaptureFields = {},
): void {
  worker.postMessage({
    type: "capture",
    seed,
    title,
    stage,
    fields: {
      size: fields.size ?? null,
      energy: fields.energy ?? null,
      context: fields.context ?? null,
      description: fields.description ?? null,
      projectId: fields.projectId ?? null,
      priority: fields.priority ?? null,
      deadline: fields.deadline ?? null,
      scheduledDate: fields.scheduledDate ?? null,
      linkUrl: fields.linkUrl ?? null,
      linkLabel: fields.linkLabel ?? null,
    },
    nowMs,
  });
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

/** S13/#111's triage mutation: edits whatever fields `edits` sets and, when
 * `destination` is `"ready"`, promotes the item, as one CAS `PATCH`. `seed`
 * mints `Core::triage`'s own queue-entry id — same caller-mints contract as
 * `actOnTask`'s. `"ready"` is triage's only destination (#360).
 *
 * `destination` is `null` (#122) for a pure field edit that leaves `stage`
 * untouched — the weekend-plans pane's do-date chip's own call shape, since
 * a promotion cannot name an item that is already `InProgress`. */
export function triageItem(
  worker: WorkerLike,
  seed: string,
  itemId: string,
  destination: "ready" | null,
  edits: TriageEdits,
  nowMs: number,
): void {
  // `edits` is forwarded as it stands, NOT normalised field by field: which
  // keys are present is the instruction (`TriageEdits`), and a `?? null` per
  // field — what this used to do — would turn every untouched field into an
  // explicit clear now that `null` means something.
  worker.postMessage({ type: "triage", seed, itemId, destination, edits, nowMs });
}

/** #355/ADR-0023's reviewed Grill outcome — the review card's Confirm
 * button submits exactly this, caller-facing convenience shape over
 * `TaskWorkerRequest`'s `"completeGrill"` variant, same split
 * `CaptureFields` documents for `captureTask`'s own optional fields. */
export interface GrillCompletion {
  transcript: string;
  summary: string;
  verdict: GrillVerdictName;
  modelProposal: string;
  appliedPatch: string;
  /** `CONTEXT.md`'s **Replace** gesture: the explicit, default-off tick.
   * `false` unless a human actually ticked it. */
  deleteUntickedPlan: boolean;
}

/** #355/ADR-0023's Grill-completion mutation: the review card's Confirm
 * button. `seed` mints `Core::complete_grill`'s own queue-entry (and
 * minted Grill) id — same caller-mints contract as `actOnTask`'s.
 *
 * `sessionSteps` is the review session's own captured snapshot of the
 * item's Steps, taken once when the interview opened
 * (`shell/useGrillTakeoverWiring.ts`'s `sessionSteps`) — never a fresh
 * read at submit time, which is what lets the core-side drift check
 * (`unticked_steps_changed`) mean anything. */
export function completeGrill(
  worker: WorkerLike,
  seed: string,
  itemId: string,
  sessionSteps: StepDTO[],
  completion: GrillCompletion,
  nowMs: number,
): void {
  worker.postMessage({
    type: "completeGrill",
    seed,
    itemId,
    sessionSteps,
    transcript: completion.transcript,
    summary: completion.summary,
    verdict: completion.verdict,
    modelProposal: completion.modelProposal,
    appliedPatch: completion.appliedPatch,
    deleteUntickedPlan: completion.deleteUntickedPlan,
    nowMs,
  });
}

/** #356/ADR-0023's draft save — the takeover's own continuous "Back or
 * close saves automatically" write, called after every completed turn.
 * Device-local: never enqueued, never touches the outbound queue. */
export function saveGrillDraft(
  worker: WorkerLike,
  itemId: string,
  turns: GrillDraftTurnDTO[],
  nowMs: number,
): void {
  worker.postMessage({ type: "saveGrillDraft", itemId, turns, nowMs });
}

/** #356's explicit, confirmed "Discard" gesture. */
export function discardGrillDraft(worker: WorkerLike, itemId: string, nowMs: number): void {
  worker.postMessage({ type: "discardGrillDraft", itemId, nowMs });
}

/** #356's resume read: one item's saved draft, if any. */
export function requestGrillDraft(worker: WorkerLike, itemId: string): void {
  worker.postMessage({ type: "getGrillDraft", itemId });
}

/** #356's bulk read: every item id carrying a draft — the Triage inbox's
 * "Resume grill" labels. */
export function requestGrillDraftItemIds(worker: WorkerLike): void {
  worker.postMessage({ type: "getGrillDraftItemIds" });
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

/** #715's toggle write: one absolute-value CAS `PUT` on that question's own
 * `settings` row, enqueued durably. `seed` mints
 * `Core::set_question_enabled`'s queue-entry id — same caller-mints
 * contract as `setBinding`'s. `question` is `StandingQuestion`'s wire
 * spelling; the seam refuses one it cannot resolve rather than minting a
 * row for it. */
export function setQuestionEnabled(
  worker: WorkerLike,
  seed: string,
  question: string,
  enabled: boolean,
  nowMs: number,
): void {
  worker.postMessage({ type: "setQuestionEnabled", seed, question, enabled, nowMs });
}

/** Every standing question's off switch (#715). */
export function requestQuestionSwitches(worker: WorkerLike): void {
  worker.postMessage({ type: "getQuestionSwitches" });
}

/** The kind registry export (#133/#140, ADR-0013). */
export function requestKindRegistry(worker: WorkerLike): void {
  worker.postMessage({ type: "getKindRegistry" });
}

/** Every rule (#140). */
export function requestRules(worker: WorkerLike): void {
  worker.postMessage({ type: "getRules" });
}

/** #140's rule create. `seed` mints `Core::create_rule`'s own queue-entry
 * id — same caller-mints contract as `actOnTask`'s. */
export function createRule(
  worker: WorkerLike,
  seed: string,
  name: string,
  eventKind: string | null,
  conditions: ConditionDTO[],
  severity: string,
  tier: TierName,
  enabled: boolean,
  nowMs: number,
): void {
  worker.postMessage({
    type: "createRule",
    seed,
    name,
    eventKind,
    conditions,
    severity,
    tier,
    enabled,
    nowMs,
  });
}

/** #140's rule patch — the enable/disable toggle, the delete, and every
 * other rule edit. `current` is the caller's own last-known copy of the row
 * (the CAS `base` a 409 is diffed against); every other field is `null` to
 * mean "leave this alone," except `enabled`, which the toggle sets directly.
 *
 * `deletedAt` follows `eventKind`'s three-way reading: **present in the
 * patch object at all** means touched, so `{ deletedAt: nowMs }` deletes and
 * `{ deletedAt: null }` un-deletes, while omitting the key leaves the flag
 * where it is. There is no `deleteRule` — a rule's deletion is one field on
 * this one CAS write, all the way down to `Core::patch_rule`. */
export function patchRule(
  worker: WorkerLike,
  seed: string,
  current: RuleDTO,
  patch: {
    name?: string | null;
    eventKind?: string | null;
    conditions?: ConditionDTO[] | null;
    severity?: string | null;
    tier?: TierName | null;
    enabled?: boolean | null;
    deletedAt?: number | null;
  },
  nowMs: number,
): void {
  worker.postMessage({
    type: "patchRule",
    seed,
    current,
    name: patch.name ?? null,
    eventKindTouched: "eventKind" in patch,
    eventKind: patch.eventKind ?? null,
    conditions: patch.conditions ?? null,
    severity: patch.severity ?? null,
    tier: patch.tier ?? null,
    enabled: patch.enabled ?? null,
    deletedAtTouched: "deletedAt" in patch,
    deletedAt: patch.deletedAt ?? null,
    nowMs,
  });
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

/** Items already grilled once and still foggy — the "triage process"
 * queue's second half (#357). */
export function requestGrillingItems(worker: WorkerLike): void {
  worker.postMessage({ type: "getGrillingItems" });
}

/** The complete retained roster — the Ledger screen's read. `nowMs` is the
 * clock the alert badge's liveness is resolved against, core-side — a
 * parameter for `requestPaneRead`'s own reason: the caller's tick is what
 * makes a re-request mean anything. */
export function requestLedger(worker: WorkerLike, nowMs: number): void {
  worker.postMessage({ type: "getLedger", nowMs });
}

/** **Recall** (#478): re-find one item across the whole retained roster by
 * remembered words or by handle. `nowMs` is the request's own clock,
 * resolving the same alert-liveness read `requestLedger` does — `search`
 * shares its corpus with `getLedger`. This function sends whatever `query`
 * it is given, blank or not — it is `useRecallWiring.ts`'s job to withhold
 * the call for an empty/whitespace-only query, not this one's; that hook is
 * this function's only caller today. `RecallOverlay` tells "nothing typed
 * yet" apart from "no results" on its own `query` prop directly
 * (`trimmed.length === 0`, checked before it ever looks at `rows`), not by
 * whether a request was sent. */
export function requestSearch(worker: WorkerLike, query: string, nowMs: number): void {
  worker.postMessage({ type: "search", query, nowMs });
}

/** Every live `Done` item — the Done screen's read. */
export function requestDone(worker: WorkerLike): void {
  worker.postMessage({ type: "getDone" });
}

/** Relation-blocked items with the reason visible — S10 (issue #108). */
export function requestBlocked(worker: WorkerLike): void {
  worker.postMessage({ type: "getBlocked" });
}

/** Items on an external wait (`Stage::Blocked`) — the last arm of the live
 * partition, read for the standing questions' inputs and nothing else
 * (#675). Deliberately not `requestBlocked`'s widening: the two are
 * different facts and CONTEXT.md keeps them apart. */
export function requestExternallyBlocked(worker: WorkerLike): void {
  worker.postMessage({ type: "getExternallyBlocked" });
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

/** #624's project create. `seed` mints `Core::create_project`'s own
 * queue-entry id — same caller-mints contract as `createRule`'s. The name is
 * trimmed and an empty one refused at the wasm seam, not here. */
export function createProject(worker: WorkerLike, seed: string, name: string, nowMs: number): void {
  worker.postMessage({ type: "createProject", seed, name, nowMs });
}

/** #625's project patch — the dossier's properties card, and every other
 * project edit. `current` is the caller's own last-known copy of the row
 * (the CAS `base` a 409 is diffed against); every field in `patch` is
 * `undefined` to mean "leave this alone," except `githubRepo`/
 * `defaultContext`, which distinguish a present-but-`null` clear from an
 * absent "don't touch" the same way `patchRule`'s `patch.eventKind` does. */
export function patchProject(
  worker: WorkerLike,
  seed: string,
  current: ProjectDTO,
  patch: {
    name?: string | null;
    githubRepo?: string | null;
    defaultContext?: string | null;
    archivedAt?: number | null;
  },
  nowMs: number,
): void {
  worker.postMessage({
    type: "patchProject",
    seed,
    current,
    name: patch.name ?? null,
    githubRepoTouched: "githubRepo" in patch,
    githubRepo: patch.githubRepo ?? null,
    defaultContextTouched: "defaultContext" in patch,
    defaultContext: patch.defaultContext ?? null,
    archivedAtTouched: "archivedAt" in patch,
    archivedAt: patch.archivedAt ?? null,
    nowMs,
  });
}

/** #626's per-project link read — the dossier aside's `requestSteps`-style
 * per-id fetch. */
export function requestProjectLinks(worker: WorkerLike, projectId: string): void {
  worker.postMessage({ type: "getProjectLinks", projectId });
}

/** #626's link create. `seed` mints `Core::create_project_link`'s own
 * queue-entry id — same caller-mints contract as `createProject`'s. The url
 * is trimmed and an empty one refused at the wasm seam, not here. */
export function createProjectLink(
  worker: WorkerLike,
  seed: string,
  projectId: string,
  url: string,
  label: string | null,
  position: number,
  nowMs: number,
): void {
  worker.postMessage({ type: "createProjectLink", seed, projectId, url, label, position, nowMs });
}

/** #626's link patch — editing, reordering and removing a link all share
 * this one call. `current` is the caller's own last-known copy of the row
 * (the CAS `base` a 409 is diffed against); every field in `patch` is
 * `undefined` to mean "leave this alone," except `label`/`removedAt`, which
 * distinguish a present-but-`null` clear from an absent "don't touch" the
 * same way `patchProject`'s `patch.githubRepo` does. */
export function patchProjectLink(
  worker: WorkerLike,
  seed: string,
  current: ProjectLinkDTO,
  patch: {
    url?: string;
    label?: string | null;
    position?: number;
    removedAt?: number | null;
  },
  nowMs: number,
): void {
  worker.postMessage({
    type: "patchProjectLink",
    seed,
    current,
    url: patch.url ?? null,
    labelTouched: "label" in patch,
    label: patch.label ?? null,
    position: patch.position ?? null,
    removedAtTouched: "removedAt" in patch,
    removedAt: patch.removedAt ?? null,
    nowMs,
  });
}

/** #627's per-project Route read — the dossier's reading column's fetch,
 * same `requestProjectLinks`-style per-id shape. */
export function requestRoute(worker: WorkerLike, projectId: string): void {
  worker.postMessage({ type: "getRoute", projectId });
}

/** #627's route patch — the dossier's reading column edits
 * destination/notes through this one call. `current` is the caller's own
 * last-known copy of the row (the CAS `base` a 409 is diffed against);
 * every field in `patch` is `undefined` to mean "leave this alone," except
 * `destination`/`notes`, which distinguish a present-but-`null` clear from
 * an absent "don't touch" the same way `patchProject`'s `patch.githubRepo`
 * does. */
export function patchRoute(
  worker: WorkerLike,
  seed: string,
  current: RouteDTO,
  patch: { destination?: string | null; notes?: string | null },
  nowMs: number,
): void {
  worker.postMessage({
    type: "patchRoute",
    seed,
    current,
    destinationTouched: "destination" in patch,
    destination: patch.destination ?? null,
    notesTouched: "notes" in patch,
    notes: patch.notes ?? null,
    nowMs,
  });
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

// -- #707's diagnostics-journal reads/writes --------------------------

export function requestDiagnosticsExport(worker: WorkerLike): void {
  worker.postMessage({ type: "getDiagnostics" });
}

/** Settings' "Clear diagnostics" button. No reply — same "fire and forget"
 * contract `protocol.ts`'s `clearDiagnostics` documents. */
export function requestClearDiagnostics(worker: WorkerLike): void {
  worker.postMessage({ type: "clearDiagnostics" });
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
