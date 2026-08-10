import type { CalendarWorkerRequest, SyncCadenceRequest, TaskWorkerRequest } from "../store/protocol";

// `core.worker.ts` wires one `PortRegistry` (ports.ts) over two independent
// request queues — the calendar binding's (#73) and the task binding's
// (#105/S7) — because the two wrap different Rust objects with their own
// independent check-out/check-in guards and gain nothing from being
// serialised against each other. `SyncCadenceRequest` (S9 round-1 review)
// reaches neither queue at all — it is intercepted directly by
// `core.worker.ts`'s dispatch, since it operates on the shared cadence/
// visibility state, not either wasm host. This is the pure dispatch logic
// that tells a request's destination apart by `type`, kept free of both the
// wasm import and `PortRegistry` itself so it is unit-testable in isolation.

type AnyWorkerRequest = CalendarWorkerRequest | TaskWorkerRequest | SyncCadenceRequest;

// A `Record` keyed by every `TaskWorkerRequest["type"]` literal, rather than
// a `Set<TaskWorkerRequest["type"]>` built from a plain array: the object
// literal below is checked against that key type at compile time, so adding
// a new request variant to the protocol without adding it here is a type
// error (a missing property), not a request that silently never routes
// (PR #171 round-1 review — the previous `Set` had no such check).
const TASK_REQUEST_TYPES: Record<TaskWorkerRequest["type"], true> = {
  pushTaskApiKey: true,
  initTaskApiKey: true,
  clearTaskApiKey: true,
  capture: true,
  act: true,
  triage: true,
  setBinding: true,
  getBindings: true,
  getFrontier: true,
  getTriageInbox: true,
  getBlocked: true,
  getSteps: true,
  getProjects: true,
  isPending: true,
  runSync: true,
  getQueueDepth: true,
  getDeadLetters: true,
  getMirrorSnapshot: true,
};

/** Same compile-time-exhaustive shape as `TASK_REQUEST_TYPES`, for the two
 * cadence-coordination messages (S9 round-1 review, PR #181). */
const SYNC_CADENCE_REQUEST_TYPES: Record<SyncCadenceRequest["type"], true> = {
  setViewVisibility: true,
  syncFocusTrigger: true,
  manualSyncTrigger: true,
};

/** Whether `request` belongs on the task binding's queue rather than the
 * calendar binding's. Never true for a `SyncCadenceRequest` — check
 * `isSyncCadenceRequest` first, the same order `core.worker.ts`'s dispatch
 * uses. */
export function isTaskWorkerRequest(request: AnyWorkerRequest): request is TaskWorkerRequest {
  return Object.hasOwn(TASK_REQUEST_TYPES, request.type);
}

/** Whether `request` is one of the two shared-cadence coordination messages
 * that reach neither wasm host — `core.worker.ts`'s dispatch intercepts
 * these itself. */
export function isSyncCadenceRequest(request: AnyWorkerRequest): request is SyncCadenceRequest {
  return Object.hasOwn(SYNC_CADENCE_REQUEST_TYPES, request.type);
}
