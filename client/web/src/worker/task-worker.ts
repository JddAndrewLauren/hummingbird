import type {
  TaskEventDTO,
  TaskItemDTO,
  TaskRunOutcomeKind,
  TaskWorkerRequest,
  TaskWorkerResponse,
} from "../store/protocol";
import { createSerialQueue } from "./serial-queue";

// The worker's half of #105/S7's task binding, kept free of the wasm import
// so vitest (node) can exercise it against a fake `TaskHostLike` — same
// discipline as `calendar-worker.ts`/`announce.ts`.

/** The slice of `hummingbird-ffi-web`'s `TaskHost` this handler needs. Every
 * method mirrors the wasm-bindgen surface exactly (see
 * `client/ffi-web/src/lib.rs`'s `wasm_bindings::TaskHost`); the JSON
 * methods resolve to the string the core serializes, not a parsed value —
 * parsing is this module's job, so the wire format is explicit in one
 * place. */
export interface TaskHostLike {
  pushApiKey(apiKey: string): void;
  clearApiKey(): void;
  capture(seed: string, title: string, stage: string, nowMs: number): Promise<string>;
  frontier(): string;
  triageInbox(): string;
  isPending(itemId: string): string;
  takeEvents(): string;
  runSync(
    nowMs: number,
    trigger: string,
    forceFullSweep: boolean,
    jitterUnit: number,
  ): Promise<string>;
}

interface RawItem {
  id: string;
  seq: number | null;
  title: string;
  description: string | null;
  stage: string;
  size: string | null;
  energy: string | null;
  context: string | null;
  priority: number;
  project_id: string | null;
  project_pos: number | null;
  due_date: string | null;
  scheduled_date: string | null;
  source: string | null;
  source_key: string | null;
  source_url: string | null;
  archived_at: number | null;
  created_at: number;
  updated_at: number;
  version: number;
}

interface RawItemListResponse {
  kind: "ok" | "busy";
  items: RawItem[];
}

interface RawIsPendingResponse {
  kind: "ok" | "busy";
  pending: boolean;
}

interface RawCaptureResponse {
  kind: "ok" | "failed" | "busy";
  id: string | null;
  error: string | null;
}

interface RawTaskEvent {
  kind: "credential_needed";
  at_ms: number;
}

interface RawRunResponse {
  kind: TaskRunOutcomeKind;
  retry_after_ms: number | null;
  active_item_count: number | null;
  // `Option<bool>` on the Rust side (`RunResponse::was_full_sweep`,
  // `client/ffi-web/src/task_host.rs`) serializes to a JSON boolean or
  // `null`, never a number — pinned by that file's own
  // `run_response_serializes_with_the_exact_keys_and_kind_literals_task_worker_ts_parses`
  // test, so this is the real wire shape, not a defensive guess at it.
  was_full_sweep: boolean | null;
  dead_lettered: number | null;
}

function mapItem(raw: RawItem): TaskItemDTO {
  return {
    id: raw.id,
    seq: raw.seq,
    title: raw.title,
    description: raw.description,
    stage: raw.stage as TaskItemDTO["stage"],
    size: raw.size as TaskItemDTO["size"],
    energy: raw.energy as TaskItemDTO["energy"],
    context: raw.context,
    priority: raw.priority,
    projectId: raw.project_id,
    projectPos: raw.project_pos,
    dueDate: raw.due_date,
    scheduledDate: raw.scheduled_date,
    source: raw.source,
    sourceKey: raw.source_key,
    sourceUrl: raw.source_url,
    archivedAt: raw.archived_at,
    createdAt: raw.created_at,
    updatedAt: raw.updated_at,
    version: raw.version,
  };
}

function mapEvents(raw: RawTaskEvent[]): TaskEventDTO[] {
  return raw.map((event) => ({ kind: event.kind, atMs: event.at_ms }));
}

/** Drains and broadcasts task events, if any. Called after every request
 * that could have produced one (today, only a `runSync` that holds on a
 * rejected credential does).
 *
 * `post` here is `PortRegistry.broadcast`, not a reply to whichever port
 * triggered the drain — deliberately: `Core::take_events` is a destructive,
 * single-reader drain (#104's own review finding), so posting only back to
 * the requesting port would let the first tab to poll silently swallow an
 * event every other connected tab needed too (#105's second load-bearing
 * handoff). Fanning it out here, at the one place every task request's
 * response already flows through, is what closes that gap without adding a
 * second broadcast path. */
function postTaskEvents(host: TaskHostLike, post: (response: TaskWorkerResponse) => void): void {
  const raw = JSON.parse(host.takeEvents()) as RawTaskEvent[];
  if (raw.length > 0) {
    post({ type: "taskEvents", events: mapEvents(raw) });
  }
}

/** Handles one `TaskWorkerRequest`, posting whatever `TaskWorkerResponse`(s)
 * it produces. Callers should go through [`createTaskRequestQueue`] rather
 * than calling this directly — see that function's own doc for why. */
export async function handleTaskRequest(
  request: TaskWorkerRequest,
  host: TaskHostLike,
  post: (response: TaskWorkerResponse) => void,
): Promise<void> {
  switch (request.type) {
    case "pushTaskApiKey":
      // Forwarded and never echoed: no branch below, or anywhere else in
      // this module, ever posts a message carrying an API key.
      host.pushApiKey(request.apiKey);
      return;
    case "clearTaskApiKey":
      // "Forget token" (#106/S8): forwarded and never acknowledged with a
      // reply — there is nothing to reply with, and the host already
      // updated its own local state before sending this.
      host.clearApiKey();
      return;
    case "capture": {
      const raw = JSON.parse(
        await host.capture(request.seed, request.title, request.stage, request.nowMs),
      ) as RawCaptureResponse;
      post({ type: "captureResult", seed: request.seed, kind: raw.kind, id: raw.id, error: raw.error });
      postTaskEvents(host, post);
      return;
    }
    case "getFrontier": {
      const raw = JSON.parse(host.frontier()) as RawItemListResponse;
      if (raw.kind === "busy") {
        // No answer, not an empty answer — busy says nothing about the
        // frontier's real contents (same contract as the calendar tile's
        // "busy", protocol.ts).
        return;
      }
      post({ type: "frontier", items: raw.items.map(mapItem) });
      return;
    }
    case "getTriageInbox": {
      const raw = JSON.parse(host.triageInbox()) as RawItemListResponse;
      if (raw.kind === "busy") {
        return;
      }
      post({ type: "triageInbox", items: raw.items.map(mapItem) });
      return;
    }
    case "isPending": {
      const raw = JSON.parse(host.isPending(request.itemId)) as RawIsPendingResponse;
      if (raw.kind === "busy") {
        return;
      }
      post({ type: "isPendingResult", itemId: request.itemId, pending: raw.pending });
      return;
    }
    case "runSync": {
      const raw = JSON.parse(
        await host.runSync(request.nowMs, request.trigger, request.forceFullSweep, request.jitterUnit),
      ) as RawRunResponse;
      post({
        type: "syncOutcome",
        kind: raw.kind,
        retryAfterMs: raw.retry_after_ms,
        activeItemCount: raw.active_item_count,
        wasFullSweep: raw.was_full_sweep,
        deadLettered: raw.dead_lettered,
      });
      postTaskEvents(host, post);
      return;
    }
  }
}

/** How long one task request may run before the queue abandons it rather
 * than stalling every request behind it (issue #95's named risk: "a hung
 * request wedges the worker"). Generous relative to a normal sync cycle
 * (a handful of HTTP round trips) so a slow-but-completing cycle is never
 * mistaken for a hang; see `serial-queue.ts` for what "abandoned" means and
 * why it is safe. */
export const TASK_REQUEST_TIMEOUT_MS = 30_000;

/** Serialises every task request into one at-a-time chain — its own queue,
 * independent of the calendar binding's (`calendar-worker.ts`'s
 * `createRequestQueue`): the two wrap different Rust objects with their own
 * independent check-out/check-in re-entrancy guards, so nothing requires
 * ordering a `capture` against a calendar poll. Built on `serial-queue.ts`'s
 * generic abandon-on-timeout queue rather than a second copy of the
 * ordering logic `createRequestQueue` already has. */
export function createTaskRequestQueue(
  host: TaskHostLike,
  post: (response: TaskWorkerResponse) => void,
): (request: TaskWorkerRequest) => Promise<void> {
  return createSerialQueue(
    (request: TaskWorkerRequest) => handleTaskRequest(request, host, post),
    {
      timeoutMs: TASK_REQUEST_TIMEOUT_MS,
      onTimeout: (request) => {
        console.error("task worker request abandoned after timeout", request.type);
      },
      onError: (request, error) => {
        console.error("task worker request failed", request.type, error);
      },
    },
  );
}
