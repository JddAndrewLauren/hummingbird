import type { CalendarWorkerRequest, TaskWorkerRequest } from "../store/protocol";

// `core.worker.ts` wires one `PortRegistry` (ports.ts) over two independent
// request queues — the calendar binding's (#73) and the task binding's
// (#105/S7) — because the two wrap different Rust objects with their own
// independent check-out/check-in guards and gain nothing from being
// serialised against each other. This is the pure dispatch logic that tells
// a request's destination queue apart by `type`, kept free of both the wasm
// import and `PortRegistry` itself so it is unit-testable in isolation.

// A `Record` keyed by every `TaskWorkerRequest["type"]` literal, rather than
// a `Set<TaskWorkerRequest["type"]>` built from a plain array: the object
// literal below is checked against that key type at compile time, so adding
// a new request variant to the protocol without adding it here is a type
// error (a missing property), not a request that silently never routes
// (PR #171 round-1 review — the previous `Set` had no such check).
const TASK_REQUEST_TYPES: Record<TaskWorkerRequest["type"], true> = {
  pushTaskApiKey: true,
  clearTaskApiKey: true,
  capture: true,
  getFrontier: true,
  getTriageInbox: true,
  isPending: true,
  runSync: true,
  getQueueDepth: true,
  getDeadLetters: true,
  getMirrorSnapshot: true,
};

/** Whether `request` belongs on the task binding's queue rather than the
 * calendar binding's. */
export function isTaskWorkerRequest(
  request: CalendarWorkerRequest | TaskWorkerRequest,
): request is TaskWorkerRequest {
  return Object.hasOwn(TASK_REQUEST_TYPES, request.type);
}
