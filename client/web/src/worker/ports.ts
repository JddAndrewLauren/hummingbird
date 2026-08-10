import type {
  CalendarWorkerRequest,
  SyncCadenceRequest,
  TaskWorkerRequest,
  WorkerResponse,
} from "../store/protocol";
import { announceReady } from "./announce";

type AnyWorkerRequest = CalendarWorkerRequest | TaskWorkerRequest | SyncCadenceRequest;

// The narrow slice of `MessagePort` the registry needs — narrow enough that
// tests can pass a plain object instead of a real port (same discipline as
// `store/worker-client.ts`'s `WorkerLike`).
export interface PortLike {
  postMessage(response: WorkerResponse): void;
  onmessage: ((event: MessageEvent<AnyWorkerRequest>) => void) | null;
  start(): void;
}

/** `port` is the port the request arrived on — S9 round-1 review: the
 * shared cadence's `setViewVisibility` needs to know WHICH view is
 * reporting, since the `SharedWorker` global scope has no `document` of its
 * own to read visibility from directly. Every other request ignores it. */
type Enqueue = (request: AnyWorkerRequest, port: PortLike) => Promise<void>;

type State =
  | { kind: "pending" }
  | { kind: "ready"; enqueue: Enqueue; coreApiVersion: () => number }
  | { kind: "failed"; message: string };

/** ADR-0010: one core in a `SharedWorker`, N connecting views. Every tab and
 * the installed PWA window connects a `MessagePort`; the registry is what
 * turns "one core" into "every view sees the same thing" — it keeps the
 * port list `core.worker.ts`'s `onconnect` grows, broadcasts every published
 * event (poll outcomes, credential events, the tile, the picker list) to
 * every connected port, and announces the "ready"/"error" handshake to only
 * the port that just connected. That last part is the fix for the sharpest
 * edge in the migration: a dedicated worker's `announceReady` posted once,
 * unprompted, at module evaluation — correct when there is exactly one
 * view, but a second view connecting after the core is already running
 * would never see a handshake that already happened. Posting it per
 * connecting port keeps the push-only, unprompted shape (see protocol.ts)
 * while covering every view, not just the first.
 *
 * **The registry is built to exist before the core does.** `core.worker.ts`
 * loads the wasm module with a dynamic `import()` so `self.onconnect` can be
 * wired synchronously, before that import resolves — the fix for PR #167
 * round-1 review blocker 1: `connect` has no platform buffering, so the
 * connect event from the very view that starts the SharedWorker is dropped
 * if `onconnect` is not already assigned when the worker's event loop
 * starts. That means `connect` can be called before the wasm core, its
 * `CalendarHost`, and the request queue exist. A port that arrives during
 * that window is queued (`pending`) rather than wired or dropped, and gets
 * its handshake retroactively — `ready` if `activate` resolves the race,
 * `error` if `activateError` does (the other half of blocker 1/2: a CSP
 * rejecting wasm compilation must reach every view as `{type: "error"}`
 * instead of hanging it on "Loading core…" forever, which is what a
 * dedicated Worker's `onerror` gave for free and a SharedWorker does not —
 * see main.tsx). Once resolved either way, the registry never reverts: a
 * wasm import does not retry mid-session.
 *
 * **Issue #195: a connecting port also gets a replay of the current
 * latest-state.** Without this, a view whose port wires up after the most
 * recent broadcast has no sync state of its own until the next cycle — and
 * if the shared 60s timer is paused (every view hidden, S9/#191), that can
 * be indefinitely. `broadcast` retains the latest message of each type in
 * `LATEST_STATE_TYPES` below, and `wire` replays whatever is retained right
 * after the `ready`/`error` handshake, before anything else can reach the
 * port.
 *
 * **The rule for `LATEST_STATE_TYPES` membership**, so the next message
 * added to the protocol has an answer: a type belongs in the set if it
 * represents a durable fact about the core's current state — one that stays
 * true until superseded by a later broadcast of the same type, and that a
 * view which missed every prior broadcast still needs to know right now
 * (`syncOutcome`'s kind/backoff/hold, `queueDepth`, `deadLetters`,
 * `taskHostUnavailable`'s not-recoverable-this-session failure). A type is
 * left out if it instead describes something that HAPPENED — a point-in-time
 * event whose meaning is tied to when it fired (`pollOutcome`, one poll
 * attempt's result; `credentialEvents`/`taskEvents`, an append-only log of
 * moments a credential broke — replaying either to a view long after the
 * fact would make a past event read as live) — or is a targeted answer to a
 * request only the asking view is waiting on (`frontier`, `triageInbox`,
 * `isPendingResult`, `calendarList`, `currentNext`, `captureResult`,
 * `mirrorSnapshot`): a view that never asked must not receive one as if it
 * had. Getting this classification wrong in either direction is the real
 * risk #195's triage called out — a one-shot event cached here replays a
 * stale moment as a live one; a durable-state type left out leaves a late
 * view stuck on stale or fabricated state until the next broadcast. */
/** The `WorkerResponse["type"]`s cached and replayed to a newly wired port —
 * see the class doc above for the membership rule. */
const LATEST_STATE_TYPES = new Set<WorkerResponse["type"]>([
  "syncOutcome",
  "queueDepth",
  "deadLetters",
  "taskHostUnavailable",
]);

export class PortRegistry {
  // Never pruned. A closed view's port is never explicitly removed from
  // this set — `postMessage` to a disconnected `MessagePort` is a silent
  // no-op per spec, and the browser tears down the whole SharedWorker
  // global scope (this registry included) once the last port disconnects,
  // so the accumulation is bounded by the core's own lifetime, not
  // unbounded. Flagged (PR #167 round-1 review, non-blocking finding 3;
  // caveat resolved by #191) as worth revisiting: the worker now broadcasts
  // sync status on every cycle rather than only on demand, so a long-lived
  // core with a drift of closed tabs pays a growing, if still harmless,
  // per-cycle fan-out — unconditionally, not just in the on-demand case
  // this used to hedge against.
  private readonly ports = new Set<PortLike>();
  private readonly pending: PortLike[] = [];
  private state: State = { kind: "pending" };
  // Issue #195: the last broadcast of each "latest-state" message type (see
  // the class doc), replayed to every newly wired port right after its
  // `ready`/`error` handshake. A type absent here has never been broadcast
  // this session, so a connecting port gets nothing for it — the same
  // "never fabricate a cycle that didn't happen" rule a fresh core gives
  // for free.
  private readonly lastByType = new Map<WorkerResponse["type"], WorkerResponse>();

  /** Wires a newly connecting port. While the core is still initializing,
   * the port is queued instead — never dropped, never wired twice. */
  connect(port: PortLike): void {
    if (this.state.kind === "pending") {
      this.pending.push(port);
      return;
    }
    this.wire(port, this.state);
  }

  /** The core finished initializing: every port already queued, and every
   * port connecting from here on, gets wired and announced ready. */
  activate(enqueue: Enqueue, coreApiVersion: () => number): void {
    const state: State = { kind: "ready", enqueue, coreApiVersion };
    this.state = state;
    for (const port of this.pending.splice(0)) {
      this.wire(port, state);
    }
  }

  /** The core failed to initialize. Every port already queued, and every
   * port connecting from here on, gets `{type: "error"}` instead of a
   * handshake that will never come. */
  activateError(message: string): void {
    this.state = { kind: "failed", message };
    for (const port of this.pending.splice(0)) {
      port.postMessage({ type: "error", message });
    }
  }

  /** Posts one published event to every wired view. A port still queued in
   * `pending` has not been wired yet — it is caught up by its own
   * `ready`/`error` once init resolves, not by a broadcast meant for views
   * already running. */
  broadcast(response: WorkerResponse): void {
    if (LATEST_STATE_TYPES.has(response.type)) {
      this.lastByType.set(response.type, response);
    }
    for (const port of this.ports) {
      port.postMessage(response);
    }
  }

  private wire(port: PortLike, state: Exclude<State, { kind: "pending" }>): void {
    if (state.kind === "failed") {
      port.postMessage({ type: "error", message: state.message });
      return;
    }
    const { enqueue, coreApiVersion } = state;
    port.onmessage = (event) => {
      void enqueue(event.data, port);
    };
    port.start();
    this.ports.add(port);
    announceReady((response) => port.postMessage(response), coreApiVersion);
    // Issue #195: replay whatever latest-state has already happened this
    // session, right after the handshake — a port that connects before
    // anything has broadcast gets nothing here, which is what keeps a core
    // that has never run a cycle reading as never-synced rather than
    // fabricating one.
    for (const response of this.lastByType.values()) {
      port.postMessage(response);
    }
  }
}
