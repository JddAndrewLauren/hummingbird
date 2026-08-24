import type { DiagnosticEventV1DTO, WebWorkerDiagnosticEvent } from "../store/protocol";

// #707's web-worker half of the shared `DiagnosticEventV1` envelope
// (`server/domain/src/diagnostics.rs`, which #711 moved it to out of
// `client/core/src/diagnostics/mod.rs` — now re-exports only). A
// Core-sourced event arrives
// already built (drained verbatim from the wasm host — see
// `diagnostics-journal.ts`); this module is what builds the envelope for
// events the SharedWorker itself produces, `source: "web-worker"`, using
// the SAME closed `DiagnosticEvent` family names #706 declared rather than
// inventing a second vocabulary (this module's own header rule: "#708 and
// #710 are explicitly forbidden from redefining an owner enum of their
// own" applies here too — every `name` below is one already in that enum).
//
// The web-worker layer has its own session, entirely separate from any
// `DiagnosticSession` a Rust core keeps: `seq` is monotonic *within one
// producer's session*, not globally, exactly as `mod.rs`'s own doc states
// ("a session outlives any single sync cycle... seq keeps counting"). Two
// sources interleaving in one journal is expected and is what `source`
// disambiguates — the journal's own storage order (an IndexedDB
// `autoIncrement` key) is what "sequence order" means for export, not
// either producer's own `seq` field. See `diagnostics-store.ts`.
//
// **Event-family mapping, spelled out because it is not literal from the
// Agent Brief's English** ("request enqueued, request dequeued, wait
// abandoned at 30 seconds, and a busy result"): the brief calls this
// vocabulary "core.*-adjacent", and the closed enum's `core.wait_started` /
// `core.acquired` / `core.busy` families are the closest fit for the JS
// serial queue's OWN analogous lifecycle (waiting for a queue turn,
// getting it, or being told the underlying host is already busy) — the
// same general shape as the wasm-level `TaskHostCore` checkout #708
// instruments, one layer up.
//
// **`operation.abandoned`, not `operation.stalled`, for the 30s
// abandonment.** Review round 1 of PR #736 caught a real name collision: the
// shared enum already uses `operation.stalled` for a STILL-RUNNING
// operation past 30s (`client/core/src/sync/cycle.rs`'s observed cycle),
// while this queue's 30s bound is the opposite fact — the queue GIVING UP
// on the wait and moving on, with no idea whether the request will ever
// settle. Folding both into one name with no discriminating field would
// have made #712's cross-host interpretation table read them as the same
// thing. `OperationAbandoned` is a new variant this slice added to
// `server/domain/src/diagnostics.rs` (see that variant's own doc) —
// amending the shared owner enum, not forking a second one, which is the
// distinction that module's header actually draws.
//
// `core.released` is deliberately NOT emitted here — release-on-settle at
// this layer was not asked for, and inventing it risks colliding with
// #708's own wasm-level `core.released`, which records the more decisive
// fact (the actual `TaskHostCore` checkout closing). If a reviewer wants
// the JS queue's own release recorded too, that is a follow-up, not a
// silent addition here.
//
// **A consequence #708 is built directly on**: this queue's own
// `core.wait_started`/`core.acquired` pairs never close with a matching
// `core.released` (the paragraph above), so a reader computing "how long
// was X held" from this journal alone cannot pair them by `source` and
// `session_id` the way it can for #708's wasm-level checkout spans — this
// layer's pairs must be read as "queue-turn" spans, distinct from
// `source: "core"`'s "wasm-checkout" spans, never merged across the two.

export const DIAGNOSTIC_EVENT_SCHEMA_VERSION = 1;

/** One producer's session-scoped identity and counters — this module's
 * analogue of `client/core/src/diagnostics/context.rs`'s
 * `DiagnosticSession`, built once per `SharedWorker` global scope
 * (`core.worker.ts` constructs exactly one, the same "one per core
 * instance" discipline `core-id.ts`'s `mintCoreId` documents). */
export interface DiagnosticsSession {
  readonly id: string;
  /** The next `seq` value, monotonically increasing for the lifetime of
   * this session — never reset per request or per cycle. */
  nextSeq(): number;
  /** Milliseconds since this session was created, from the caller's own
   * clock — this module samples nothing itself. */
  elapsedMs(nowMs: number): number;
}

const SESSION_ID_LENGTH = 32;

/** Mints this session's own id. Deliberately not `worker/core-id.ts`'s
 * `mintCoreId`: that function is documented for one specific diagnostic (a
 * view's `ready` handshake, compared "two views showing the same id share
 * one core") and reusing it here for an unrelated identity would make a
 * future reader wonder which of two things a "core id" collision meant.
 * Same defensive shape regardless — `crypto.randomUUID` is secure-context
 * only, and a diagnostic must never be able to throw during a
 * `SharedWorker`'s synchronous startup (see `core-id.ts`'s own doc for why
 * that failure mode is the one this guards). */
function mintSessionId(source: Crypto | undefined = globalThis.crypto): string {
  if (source !== undefined && "randomUUID" in source) {
    return source.randomUUID();
  }
  let id = "";
  while (id.length < SESSION_ID_LENGTH) {
    id += Math.random().toString(16).slice(2);
  }
  return id.slice(0, SESSION_ID_LENGTH);
}

/** Builds one session, anchored to `originMs` (the caller's own clock at
 * the moment the `SharedWorker` activates) so `elapsedMs` never samples a
 * second clock of its own. */
export function createDiagnosticsSession(
  originMs: number,
  idSource: Crypto | undefined = globalThis.crypto,
): DiagnosticsSession {
  const id = mintSessionId(idSource);
  let seq = 0;
  return {
    id,
    nextSeq: () => {
      seq += 1;
      return seq;
    },
    elapsedMs: (nowMs) => Math.max(0, nowMs - originMs),
  };
}

// `event` is `WebWorkerDiagnosticEvent`, not `DiagnosticEventV1DTO["event"]`:
// the latter accepts any `{name}` object with an optional `unknown` payload,
// which is precisely how this module shipped a payload-free `core.busy` after
// #708 made that variant require one — green through typecheck, lint, the web
// suite and `cargo test`, and rejected only by
// `serde_json::from_str::<DiagnosticEvent>`. See `WebWorkerDiagnosticEvent`'s
// own doc in `../store/protocol`.
function envelope(
  session: DiagnosticsSession,
  nowMs: number,
  event: WebWorkerDiagnosticEvent,
): DiagnosticEventV1DTO {
  return {
    schema_version: DIAGNOSTIC_EVENT_SCHEMA_VERSION,
    seq: session.nextSeq(),
    wall_clock_ms: nowMs,
    elapsed_ms: session.elapsedMs(nowMs),
    session_id: session.id,
    source: "web-worker",
    cycle_id: null,
    // #708 lands operation correlation; this slice never has one to carry
    // (see `core.worker.ts`'s own note on the fact).
    operation_id: null,
    request_id: null,
    event,
  };
}

/** A task request was added to the serial queue's tail chain
 * (`serial-queue.ts`'s `onEnqueue`) — its wait for a turn begins here. */
export function requestEnqueuedEvent(session: DiagnosticsSession, nowMs: number): DiagnosticEventV1DTO {
  return envelope(session, nowMs, { name: "core.wait_started" });
}

/** The queue reached this request's turn and started running its handler
 * (`serial-queue.ts`'s `onDequeue`). */
export function requestDequeuedEvent(session: DiagnosticsSession, nowMs: number): DiagnosticEventV1DTO {
  return envelope(session, nowMs, { name: "core.acquired" });
}

/** `serial-queue.ts`'s `withTimeout` gave up waiting on the request at
 * `TASK_REQUEST_TIMEOUT_MS` (30s) and moved the queue on —
 * `operation.abandoned`, a family this slice added to the shared enum (see
 * the module doc's "review round 1" note): the still-running
 * `operation.stalled` #705's plan names for that same 30s figure is a
 * different fact from a queue that has stopped waiting. */
export function requestAbandonedEvent(session: DiagnosticsSession, nowMs: number): DiagnosticEventV1DTO {
  return envelope(session, nowMs, { name: "operation.abandoned" });
}

/** A task request's own result carried `kind: "busy"` — the underlying
 * wasm host was already checked out when this request reached it.
 *
 * **`owner: null`, and why this layer cannot do better (#708 review round
 * 2).** #708 gave the shared `core.busy` variant an `owner` naming the
 * `CoreOwner` that holds the wasm checkout. This layer cannot supply it: the
 * only record of the holder is the private `holder` cell inside
 * `client/ffi-web/src/task_host.rs`'s `TaskCoreCell`, and the sole signal
 * that reaches here is a worker response's `kind: "busy"` string — not one of
 * those response DTOs carries an owner (`task-worker.ts`'s
 * `postAndObserveBusy` sees the response and nothing else). So `null` is the
 * honest value, and the shared contract types the field as optional for
 * exactly this producer: `null` means "this writer could not see the holder",
 * never "nobody held it".
 *
 * Nothing is lost by it. The same refused checkout also produces a
 * `source: "core"` `core.busy` row *with* the owner, drained into this same
 * journal by `drainAroundRequest`. This row is the queue-level observation
 * ("a request was refused here"); that row is the authoritative holder. A
 * reader joins them under `TaskCoreCell`'s `(source, session_id)` scoping
 * rule, never by merging the two spans.
 *
 * The `payload` key is emitted explicitly rather than omitted: the shared
 * enum is adjacently tagged, so an absent `payload` on a struct variant is a
 * hard deserialization failure — see `server/domain/src/diagnostics.rs`'s
 * header rule 2. */
export function requestBusyEvent(session: DiagnosticsSession, nowMs: number): DiagnosticEventV1DTO {
  return envelope(session, nowMs, { name: "core.busy", payload: { owner: null } });
}

/** A browser online/offline transition, or a visibility-state change that
 * prompted re-checking `navigator.onLine` (mobile browsers do not always
 * fire `online`/`offline` reliably, so visibility changes are a second
 * trigger for the same fact — see `core.worker.ts`). `online` is the one
 * field the shared contract's `NetworkChanged` payload carries; richer
 * Network Information API attributes (`effectiveType`, `downlink`, …) have
 * nowhere to go in the current envelope — see this repo's issue #707
 * finding on that gap. */
export function networkChangedEvent(
  session: DiagnosticsSession,
  nowMs: number,
  online: boolean,
): DiagnosticEventV1DTO {
  return envelope(session, nowMs, { name: "network.changed", payload: { online } });
}
