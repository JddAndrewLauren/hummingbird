import type { DiagnosticEventV1DTO } from "../store/protocol";
import {
  createDiagnosticsSession,
  networkChangedEvent,
  requestAbandonedEvent,
  requestBusyEvent,
  requestDequeuedEvent,
  requestEnqueuedEvent,
  type DiagnosticsSession,
} from "./diagnostics-events";
import { createDiagnosticsStore, type DiagnosticsStoreLike } from "./diagnostics-store";

// #707's single entry point: one `DiagnosticsJournal` per `SharedWorker`
// global scope, constructed once in `core.worker.ts` alongside `registry`
// and `visibility` (the same "one per core instance" discipline those two
// already follow). Everything else in the `diagnostics-*` family
// (`-events.ts`'s builders, `-store.ts`'s IndexedDB adapter,
// `-retention.ts`'s pure policy) is a collaborator this module wires
// together; nothing outside this file needs to know how a "busy" result
// becomes a `core.busy` envelope or how retention is enforced.
//
// **Every method here swallows its own failures** — a `DiagnosticsStoreLike`
// already never throws (see `diagnostics-store.ts`'s module doc), and the
// wasm drain call below is wrapped the same way, so a caller (`task-worker.ts`,
// `core.worker.ts`) never has to guard a diagnostics call the way it guards
// a real mutation.

/** How often `drainWhileOutstanding` polls the wasm host for new Core
 * events while an async task request is still in flight — the Agent
 * Brief's own number, chosen so a request that hangs for minutes still
 * leaves a trail rather than one silent gap between "before" and "after". */
const DRAIN_INTERVAL_MS = 250;

export interface DiagnosticsJournal {
  recordEnqueued(nowMs: number): void;
  recordDequeued(nowMs: number): void;
  recordAbandoned(nowMs: number): void;
  recordBusy(nowMs: number): void;
  recordNetworkChanged(nowMs: number, online: boolean): void;
  /** Wraps one task request's handler: drains the wasm host's Core-sourced
   * events immediately before and after `run`, and every `DRAIN_INTERVAL_MS`
   * while `run`'s promise is still pending. `drainHost` returns a JSON array
   * string of new events, or `undefined`/`null` when the host has nothing
   * to drain (including a pre-#708 host that does not implement the method
   * at all — see `task-worker.ts`'s `TaskHostLike.drainDiagnostics`). */
  drainAroundRequest<T>(run: () => Promise<T>, drainHost: () => string | null | undefined, nowMs: () => number): Promise<T>;
  export(): Promise<{ events: DiagnosticEventV1DTO[]; droppedCount: number }>;
  clear(): Promise<void>;
}

function parseHostEvents(raw: string | null | undefined): DiagnosticEventV1DTO[] {
  if (raw === null || raw === undefined) {
    return [];
  }
  try {
    const parsed: unknown = JSON.parse(raw);
    return Array.isArray(parsed) ? (parsed as DiagnosticEventV1DTO[]) : [];
  } catch {
    // A malformed drain payload is itself exactly the kind of storage
    // failure this journal must swallow rather than propagate — see the
    // module doc.
    return [];
  }
}

class DiagnosticsJournalImpl implements DiagnosticsJournal {
  constructor(
    private readonly store: DiagnosticsStoreLike,
    private readonly session: DiagnosticsSession,
  ) {}

  private write(events: DiagnosticEventV1DTO[], nowMs: number): void {
    if (events.length === 0) {
      return;
    }
    // Fire-and-forget from every call site's point of view: a diagnostic
    // write's own promise is never awaited by a caller mid-request, since
    // that would make sync/capture/triage latency depend on IndexedDB.
    // `.catch` here is defence in depth, not the primary contract —
    // `DiagnosticsStoreLike` is documented to never reject — but a broken
    // or test-doubled store must still never surface as an unhandled
    // rejection, which is exactly the kind of "a diagnostic storage
    // failure is swallowed" the Agent Brief asks for.
    this.store.append(events, nowMs).catch(() => {});
  }

  recordEnqueued(nowMs: number): void {
    this.write([requestEnqueuedEvent(this.session, nowMs)], nowMs);
  }

  recordDequeued(nowMs: number): void {
    this.write([requestDequeuedEvent(this.session, nowMs)], nowMs);
  }

  recordAbandoned(nowMs: number): void {
    this.write([requestAbandonedEvent(this.session, nowMs)], nowMs);
  }

  recordBusy(nowMs: number): void {
    this.write([requestBusyEvent(this.session, nowMs)], nowMs);
  }

  recordNetworkChanged(nowMs: number, online: boolean): void {
    this.write([networkChangedEvent(this.session, nowMs, online)], nowMs);
  }

  private drainHostOnce(drainHost: () => string | null | undefined, nowMs: number): void {
    const events = parseHostEvents(drainHost());
    this.write(events, nowMs);
  }

  async drainAroundRequest<T>(
    run: () => Promise<T>,
    drainHost: () => string | null | undefined,
    nowMs: () => number,
  ): Promise<T> {
    this.drainHostOnce(drainHost, nowMs());
    const timer = setInterval(() => this.drainHostOnce(drainHost, nowMs()), DRAIN_INTERVAL_MS);
    try {
      return await run();
    } finally {
      clearInterval(timer);
      this.drainHostOnce(drainHost, nowMs());
    }
  }

  export(): Promise<{ events: DiagnosticEventV1DTO[]; droppedCount: number }> {
    return this.store.exportAll();
  }

  clear(): Promise<void> {
    return this.store.clear();
  }
}

/** Builds the one journal a `SharedWorker` global scope owns. `store` and
 * `originMs` are both injectable so a test can exercise this without real
 * IndexedDB or a real clock — `core.worker.ts` calls this with
 * `createDiagnosticsStore()` (the real IndexedDB adapter) and `Date.now()`. */
export function createDiagnosticsJournal(
  originMs: number,
  store: DiagnosticsStoreLike = createDiagnosticsStore(),
): DiagnosticsJournal {
  return new DiagnosticsJournalImpl(store, createDiagnosticsSession(originMs));
}
