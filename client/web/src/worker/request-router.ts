import type { CalendarWorkerRequest, TaskWorkerRequest } from "../store/protocol";

// `core.worker.ts` wires one `PortRegistry` (ports.ts) over two independent
// request queues — the calendar binding's (#73) and the task binding's
// (#105/S7) — because the two wrap different Rust objects with their own
// independent check-out/check-in guards and gain nothing from being
// serialised against each other. This is the pure dispatch logic that tells
// a request's destination queue apart by `type`, kept free of both the wasm
// import and `PortRegistry` itself so it is unit-testable in isolation.

const TASK_REQUEST_TYPES: ReadonlySet<TaskWorkerRequest["type"]> = new Set([
  "pushTaskApiKey",
  "capture",
  "getFrontier",
  "getTriageInbox",
  "isPending",
  "runSync",
]);

/** Whether `request` belongs on the task binding's queue rather than the
 * calendar binding's. */
export function isTaskWorkerRequest(
  request: CalendarWorkerRequest | TaskWorkerRequest,
): request is TaskWorkerRequest {
  return TASK_REQUEST_TYPES.has(request.type as TaskWorkerRequest["type"]);
}
