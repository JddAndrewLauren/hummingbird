import { describe, expect, it, vi } from "vitest";
import type { CalendarWorkerRequest, WorkerResponse } from "../store/protocol";
import { PortRegistry, type PortLike } from "./ports";

// ADR-0010: one core, N connecting views (`MessagePort`s). A fake port —
// same discipline as announce.ts/calendar-worker.ts's fakes, so this exercises
// the connect/broadcast behaviour without a real SharedWorker.
function fakePort(): PortLike & { postMessage: ReturnType<typeof vi.fn> } {
  return {
    onmessage: null,
    postMessage: vi.fn(),
    start: vi.fn(),
  };
}

// #172: every handshake now carries the core instance id and this view's
// connect ordinal. A fixed id here is exactly why `PortRegistry` takes it
// rather than minting one — the same caller-injected-randomness idiom
// `client/core`'s `seed`/`Now` follow.
const CORE_ID = "3f2a1b8c";

function ready(apiVersion: number, viewOrdinal: number): WorkerResponse {
  return { type: "ready", apiVersion, coreId: CORE_ID, viewOrdinal };
}

describe("PortRegistry", () => {
  describe("once activated", () => {
    it("announces ready to a newly connecting port", () => {
      const registry = new PortRegistry(CORE_ID);
      registry.activate(vi.fn().mockResolvedValue(undefined), () => 3);
      const port = fakePort();

      registry.connect(port);

      expect(port.postMessage).toHaveBeenCalledWith(ready(3, 1));
    });

    it("broadcasts a published event to every connected port", () => {
      const registry = new PortRegistry(CORE_ID);
      registry.activate(vi.fn().mockResolvedValue(undefined), () => 1);
      const first = fakePort();
      const second = fakePort();
      registry.connect(first);
      registry.connect(second);

      const outcome: WorkerResponse = { type: "pollOutcome", outcome: "succeeded" };
      registry.broadcast(outcome);

      expect(first.postMessage).toHaveBeenCalledWith(outcome);
      expect(second.postMessage).toHaveBeenCalledWith(outcome);
    });

    it("a port connecting after another is already running still gets its own ready handshake", () => {
      const registry = new PortRegistry(CORE_ID);
      registry.activate(vi.fn().mockResolvedValue(undefined), () => 5);
      const first = fakePort();
      registry.connect(first);
      first.postMessage.mockClear();

      const second = fakePort();
      registry.connect(second);

      expect(second.postMessage).toHaveBeenCalledWith(ready(5, 2));
      // The first view's handshake already happened — connecting a second
      // view must not re-announce to it.
      expect(first.postMessage).not.toHaveBeenCalled();
    });

    it("routes an incoming message on any connected port through the shared queue", () => {
      const enqueue = vi.fn().mockResolvedValue(undefined);
      const registry = new PortRegistry(CORE_ID);
      registry.activate(enqueue, () => 1);
      const port = fakePort();
      registry.connect(port);

      const request: CalendarWorkerRequest = { type: "listCalendars" };
      port.onmessage?.({ data: request } as MessageEvent<CalendarWorkerRequest>);

      expect(enqueue).toHaveBeenCalledWith(request, port);
    });

    it("tells two connected ports' messages apart by the port each arrived on — S9's shared-cadence visibility tracking depends on this", () => {
      const enqueue = vi.fn().mockResolvedValue(undefined);
      const registry = new PortRegistry(CORE_ID);
      registry.activate(enqueue, () => 1);
      const first = fakePort();
      const second = fakePort();
      registry.connect(first);
      registry.connect(second);

      const request: CalendarWorkerRequest = { type: "listCalendars" };
      first.onmessage?.({ data: request } as MessageEvent<CalendarWorkerRequest>);
      second.onmessage?.({ data: request } as MessageEvent<CalendarWorkerRequest>);

      expect(enqueue).toHaveBeenNthCalledWith(1, request, first);
      expect(enqueue).toHaveBeenNthCalledWith(2, request, second);
    });

    it("starts a connecting port so its queued incoming messages dispatch", () => {
      const registry = new PortRegistry(CORE_ID);
      registry.activate(vi.fn().mockResolvedValue(undefined), () => 1);
      const port = fakePort();

      registry.connect(port);

      expect(port.start).toHaveBeenCalled();
    });
  });

  describe("cache-and-replay of latest-state broadcasts (issue #195)", () => {
    it("replays the last sync outcome to a port connecting after it was broadcast", () => {
      const registry = new PortRegistry(CORE_ID);
      registry.activate(vi.fn().mockResolvedValue(undefined), () => 1);
      const first = fakePort();
      registry.connect(first);

      const outcome: WorkerResponse = {
        type: "syncOutcome",
        kind: "completed",
        retryAfterMs: null,
        activeItemCount: 3,
        wasFullSweep: false,
        deadLettered: 0,
        atMs: 5_000,
      };
      registry.broadcast(outcome);

      const second = fakePort();
      registry.connect(second);

      expect(second.postMessage).toHaveBeenCalledWith(outcome);
    });

    it("does not replay anything to a port connecting before any broadcast — a core that never ran a cycle stays never-synced", () => {
      const registry = new PortRegistry(CORE_ID);
      registry.activate(vi.fn().mockResolvedValue(undefined), () => 1);
      const port = fakePort();

      registry.connect(port);

      expect(port.postMessage).toHaveBeenCalledTimes(1);
      expect(port.postMessage).toHaveBeenCalledWith(ready(1, 1));
    });

    it("does not replay a one-shot broadcast (e.g. pollOutcome) to a later-connecting port", () => {
      const registry = new PortRegistry(CORE_ID);
      registry.activate(vi.fn().mockResolvedValue(undefined), () => 1);
      const first = fakePort();
      registry.connect(first);

      registry.broadcast({ type: "pollOutcome", outcome: "succeeded" });

      const second = fakePort();
      registry.connect(second);

      expect(second.postMessage).toHaveBeenCalledTimes(1);
      expect(second.postMessage).toHaveBeenCalledWith(ready(1, 2));
    });

    it("replays the latest broadcast of a type, not the first, once it has changed", () => {
      const registry = new PortRegistry(CORE_ID);
      registry.activate(vi.fn().mockResolvedValue(undefined), () => 1);
      const first = fakePort();
      registry.connect(first);

      registry.broadcast({ type: "queueDepth", depth: 5 });
      registry.broadcast({ type: "queueDepth", depth: 2 });

      const second = fakePort();
      registry.connect(second);

      expect(second.postMessage).toHaveBeenCalledWith({ type: "queueDepth", depth: 2 });
      expect(second.postMessage).not.toHaveBeenCalledWith({ type: "queueDepth", depth: 5 });
    });

    it("replays every distinct latest-state type broadcast so far, not just the most recent one", () => {
      const registry = new PortRegistry(CORE_ID);
      registry.activate(vi.fn().mockResolvedValue(undefined), () => 1);
      const first = fakePort();
      registry.connect(first);

      registry.broadcast({ type: "queueDepth", depth: 5 });
      registry.broadcast({
        type: "deadLetters",
        entries: [],
      });

      const second = fakePort();
      registry.connect(second);

      expect(second.postMessage).toHaveBeenCalledWith({ type: "queueDepth", depth: 5 });
      expect(second.postMessage).toHaveBeenCalledWith({ type: "deadLetters", entries: [] });
    });

    // The defect this closes: during an ADR-0007 backoff every 60s tick
    // broadcasts `"skipped"`. Caching those replaced the real failure in the
    // replay cache, so a view opened mid-outage replayed a `"skipped"`,
    // dropped it as uninformative (`store/worker-client.ts`), and rendered
    // "Not yet synced" while every view already running still read "Stale" —
    // the opposite of ADR-0010's one-status-per-origin.
    it("keeps the last informative outcome cached when a backed-off cycle reports skipped", () => {
      const registry = new PortRegistry(CORE_ID);
      registry.activate(vi.fn().mockResolvedValue(undefined), () => 1);
      registry.connect(fakePort());

      const failure: WorkerResponse = {
        type: "syncOutcome",
        kind: "pull_failed",
        retryAfterMs: 30_000,
        activeItemCount: 0,
        wasFullSweep: false,
        deadLettered: 0,
        atMs: 5_000,
      };
      const skipped: WorkerResponse = {
        type: "syncOutcome",
        kind: "skipped",
        retryAfterMs: 30_000,
        activeItemCount: 0,
        wasFullSweep: false,
        deadLettered: 0,
        atMs: 65_000,
      };
      registry.broadcast(failure);
      registry.broadcast(skipped);

      const late = fakePort();
      registry.connect(late);

      expect(late.postMessage).toHaveBeenCalledWith(failure);
      expect(late.postMessage).not.toHaveBeenCalledWith(skipped);
    });

    it("caches nothing when only non-attempts have been broadcast — never-synced is then the truth", () => {
      const registry = new PortRegistry(CORE_ID);
      registry.activate(vi.fn().mockResolvedValue(undefined), () => 1);
      registry.connect(fakePort());

      registry.broadcast({
        type: "syncOutcome",
        kind: "busy",
        retryAfterMs: null,
        activeItemCount: 0,
        wasFullSweep: false,
        deadLettered: 0,
        atMs: 5_000,
      });

      const late = fakePort();
      registry.connect(late);

      expect(late.postMessage).toHaveBeenCalledTimes(1);
      expect(late.postMessage).toHaveBeenCalledWith(ready(1, 2));
    });

    it("still broadcasts a skipped outcome live — only the replay cache skips it", () => {
      const registry = new PortRegistry(CORE_ID);
      registry.activate(vi.fn().mockResolvedValue(undefined), () => 1);
      const live = fakePort();
      registry.connect(live);

      const skipped: WorkerResponse = {
        type: "syncOutcome",
        kind: "skipped",
        retryAfterMs: 30_000,
        activeItemCount: 0,
        wasFullSweep: false,
        deadLettered: 0,
        atMs: 65_000,
      };
      registry.broadcast(skipped);

      expect(live.postMessage).toHaveBeenCalledWith(skipped);
    });
  });

  describe("before the core has finished initializing", () => {
    // The regression this covers: core.worker.ts's wasm import is async, so
    // the view that STARTS the SharedWorker can connect before `activate` is
    // ever called. `connect` has no platform buffering — a dropped
    // `onconnect` here is a view stuck on "Loading core…" forever (PR #167
    // round-1 review, blocker 1).
    it("queues a connecting port rather than dropping it", () => {
      const registry = new PortRegistry(CORE_ID);
      const port = fakePort();

      registry.connect(port);

      expect(port.postMessage).not.toHaveBeenCalled();
      expect(port.onmessage).toBeNull();
    });

    it("delivers the queued port's ready handshake once the core activates", () => {
      const registry = new PortRegistry(CORE_ID);
      const port = fakePort();
      registry.connect(port);

      registry.activate(vi.fn().mockResolvedValue(undefined), () => 7);

      expect(port.postMessage).toHaveBeenCalledWith(ready(7, 1));
      expect(port.start).toHaveBeenCalled();
    });

    it("wires every port that connected while init was in flight, in order", () => {
      const registry = new PortRegistry(CORE_ID);
      const first = fakePort();
      const second = fakePort();
      registry.connect(first);
      registry.connect(second);

      registry.activate(vi.fn().mockResolvedValue(undefined), () => 1);

      expect(first.postMessage).toHaveBeenCalledWith(ready(1, 1));
      expect(second.postMessage).toHaveBeenCalledWith(ready(1, 2));
    });

    it("routes a queued port's requests through the queue supplied at activate", () => {
      const enqueue = vi.fn().mockResolvedValue(undefined);
      const registry = new PortRegistry(CORE_ID);
      const port = fakePort();
      registry.connect(port);

      registry.activate(enqueue, () => 1);
      const request: CalendarWorkerRequest = { type: "listCalendars" };
      port.onmessage?.({ data: request } as MessageEvent<CalendarWorkerRequest>);

      expect(enqueue).toHaveBeenCalledWith(request, port);
    });
  });

  describe("when the core fails to initialize", () => {
    // The other half of blocker 1/2: a CSP rejecting wasm compilation (or
    // any other init failure) must reach every view as `{type: "error"}`,
    // not silently hang them on "Loading core…" — this is the worker-side
    // fallback the dedicated-Worker wiring got for free from `onerror` and
    // a SharedWorker does not.
    it("posts error to a port that was queued before the failure", () => {
      const registry = new PortRegistry(CORE_ID);
      const port = fakePort();
      registry.connect(port);

      registry.activateError("wasm init failed");

      expect(port.postMessage).toHaveBeenCalledWith({
        type: "error",
        message: "wasm init failed",
      });
    });

    it("posts error immediately to a port connecting after the failure", () => {
      const registry = new PortRegistry(CORE_ID);
      registry.activateError("wasm init failed");

      const port = fakePort();
      registry.connect(port);

      expect(port.postMessage).toHaveBeenCalledWith({
        type: "error",
        message: "wasm init failed",
      });
    });

    // #707, review round 1: "the journal is unexportable exactly when the
    // core never reaches ready — which is one of the main situations an
    // operator needs the journal." A `DiagnosticsPortHandler` is what a
    // failed core still serves, on both of the two paths a port can reach
    // "failed" through (queued-then-failed, and connecting-after-failure).
    describe("a DiagnosticsPortHandler still serves the two diagnostics messages", () => {
      function fakeDiagnosticsHandler() {
        return {
          isDiagnosticsRequest: vi.fn(
            (request: { type: string }) => request.type === "getDiagnostics" || request.type === "clearDiagnostics",
          ),
          handle: vi.fn(),
        };
      }

      it("wires onmessage to the handler for a port queued before the failure", () => {
        const diagnostics = fakeDiagnosticsHandler();
        const registry = new PortRegistry(CORE_ID, diagnostics);
        const port = fakePort();
        registry.connect(port);

        registry.activateError("wasm init failed");
        port.onmessage?.({ data: { type: "getDiagnostics" } } as MessageEvent);

        expect(diagnostics.handle).toHaveBeenCalledWith({ type: "getDiagnostics" }, port);
      });

      it("wires onmessage to the handler for a port connecting after the failure", () => {
        const diagnostics = fakeDiagnosticsHandler();
        const registry = new PortRegistry(CORE_ID, diagnostics);
        registry.activateError("wasm init failed");

        const port = fakePort();
        registry.connect(port);
        port.onmessage?.({ data: { type: "clearDiagnostics" } } as MessageEvent);

        expect(diagnostics.handle).toHaveBeenCalledWith({ type: "clearDiagnostics" }, port);
      });

      it("never routes a non-diagnostics message to the handler on a failed core", () => {
        const diagnostics = fakeDiagnosticsHandler();
        const registry = new PortRegistry(CORE_ID, diagnostics);
        registry.activateError("wasm init failed");

        const port = fakePort();
        registry.connect(port);
        port.onmessage?.({ data: { type: "getFrontier" } } as MessageEvent);

        expect(diagnostics.handle).not.toHaveBeenCalled();
      });

      it("without a handler supplied, a failed core's port drops every message as before — the pre-#707 behaviour, unchanged", () => {
        const registry = new PortRegistry(CORE_ID);
        registry.activateError("wasm init failed");

        const port = fakePort();
        registry.connect(port);

        expect(() =>
          port.onmessage?.({ data: { type: "getDiagnostics" } } as MessageEvent),
        ).not.toThrow();
      });
    });
  });

  // Issue #172: ADR-0010's own probe. The signal a person reads in two
  // windows is the pair (core instance id, view ordinal) — never
  // `ports.size`, which is never pruned and so cannot tell "two live views"
  // from "one tab opened twice".
  describe("the core identity every handshake carries", () => {
    it("gives every view the SAME core instance id — the signal that proves sharing", () => {
      const registry = new PortRegistry(CORE_ID);
      registry.activate(vi.fn().mockResolvedValue(undefined), () => 1);
      const first = fakePort();
      const second = fakePort();

      registry.connect(first);
      registry.connect(second);

      for (const port of [first, second]) {
        expect(port.postMessage).toHaveBeenCalledWith(
          expect.objectContaining({ type: "ready", coreId: CORE_ID }),
        );
      }
    });

    it("counts view ordinals from 1, one per connect", () => {
      const registry = new PortRegistry(CORE_ID);
      registry.activate(vi.fn().mockResolvedValue(undefined), () => 1);
      const ports = [fakePort(), fakePort(), fakePort()];

      for (const port of ports) {
        registry.connect(port);
      }

      expect(ports.map((port) => port.postMessage.mock.calls[0][0].viewOrdinal)).toEqual([1, 2, 3]);
    });

    // The reason the counter is minted in `connect` and not `wire`: a port
    // that arrives during the wasm import (`pending`) is wired later, by
    // `activate`'s drain, and must still carry the ordinal its ARRIVAL
    // earned. Renumbering at wire time would put the view that started the
    // core after every view that connected while it was still loading.
    it("keeps a port's arrival order when it connected before the core was ready", () => {
      const registry = new PortRegistry(CORE_ID);
      const early = fakePort();
      registry.connect(early);

      registry.activate(vi.fn().mockResolvedValue(undefined), () => 1);
      const late = fakePort();
      registry.connect(late);

      expect(early.postMessage).toHaveBeenCalledWith(ready(1, 1));
      expect(late.postMessage).toHaveBeenCalledWith(ready(1, 2));
    });

    // The refutation side, and what makes the diagnostic readable at all: a
    // second registry is a second core (a second `SharedWorker` global
    // scope), and it announces a different id and its own ordinal count
    // from 1 — which is exactly what an unshared PWA window would look
    // like on screen.
    it("a second core announces a different id and restarts its own ordinals", () => {
      const first = new PortRegistry(CORE_ID);
      first.activate(vi.fn().mockResolvedValue(undefined), () => 1);
      first.connect(fakePort());
      const second = new PortRegistry("aa11bb22");
      second.activate(vi.fn().mockResolvedValue(undefined), () => 1);

      const port = fakePort();
      second.connect(port);

      expect(port.postMessage).toHaveBeenCalledWith(
        expect.objectContaining({ coreId: "aa11bb22", viewOrdinal: 1 }),
      );
    });
  });
});
