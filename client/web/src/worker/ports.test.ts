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

describe("PortRegistry", () => {
  describe("once activated", () => {
    it("announces ready to a newly connecting port", () => {
      const registry = new PortRegistry();
      registry.activate(vi.fn().mockResolvedValue(undefined), () => 3);
      const port = fakePort();

      registry.connect(port);

      expect(port.postMessage).toHaveBeenCalledWith({ type: "ready", apiVersion: 3 });
    });

    it("broadcasts a published event to every connected port", () => {
      const registry = new PortRegistry();
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
      const registry = new PortRegistry();
      registry.activate(vi.fn().mockResolvedValue(undefined), () => 5);
      const first = fakePort();
      registry.connect(first);
      first.postMessage.mockClear();

      const second = fakePort();
      registry.connect(second);

      expect(second.postMessage).toHaveBeenCalledWith({ type: "ready", apiVersion: 5 });
      // The first view's handshake already happened — connecting a second
      // view must not re-announce to it.
      expect(first.postMessage).not.toHaveBeenCalled();
    });

    it("routes an incoming message on any connected port through the shared queue", () => {
      const enqueue = vi.fn().mockResolvedValue(undefined);
      const registry = new PortRegistry();
      registry.activate(enqueue, () => 1);
      const port = fakePort();
      registry.connect(port);

      const request: CalendarWorkerRequest = { type: "listCalendars" };
      port.onmessage?.({ data: request } as MessageEvent<CalendarWorkerRequest>);

      expect(enqueue).toHaveBeenCalledWith(request, port);
    });

    it("tells two connected ports' messages apart by the port each arrived on — S9's shared-cadence visibility tracking depends on this", () => {
      const enqueue = vi.fn().mockResolvedValue(undefined);
      const registry = new PortRegistry();
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
      const registry = new PortRegistry();
      registry.activate(vi.fn().mockResolvedValue(undefined), () => 1);
      const port = fakePort();

      registry.connect(port);

      expect(port.start).toHaveBeenCalled();
    });
  });

  describe("cache-and-replay of latest-state broadcasts (issue #195)", () => {
    it("replays the last sync outcome to a port connecting after it was broadcast", () => {
      const registry = new PortRegistry();
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
      const registry = new PortRegistry();
      registry.activate(vi.fn().mockResolvedValue(undefined), () => 1);
      const port = fakePort();

      registry.connect(port);

      expect(port.postMessage).toHaveBeenCalledTimes(1);
      expect(port.postMessage).toHaveBeenCalledWith({ type: "ready", apiVersion: 1 });
    });

    it("does not replay a one-shot broadcast (e.g. pollOutcome) to a later-connecting port", () => {
      const registry = new PortRegistry();
      registry.activate(vi.fn().mockResolvedValue(undefined), () => 1);
      const first = fakePort();
      registry.connect(first);

      registry.broadcast({ type: "pollOutcome", outcome: "succeeded" });

      const second = fakePort();
      registry.connect(second);

      expect(second.postMessage).toHaveBeenCalledTimes(1);
      expect(second.postMessage).toHaveBeenCalledWith({ type: "ready", apiVersion: 1 });
    });

    it("replays the latest broadcast of a type, not the first, once it has changed", () => {
      const registry = new PortRegistry();
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
      const registry = new PortRegistry();
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
  });

  describe("before the core has finished initializing", () => {
    // The regression this covers: core.worker.ts's wasm import is async, so
    // the view that STARTS the SharedWorker can connect before `activate` is
    // ever called. `connect` has no platform buffering — a dropped
    // `onconnect` here is a view stuck on "Loading core…" forever (PR #167
    // round-1 review, blocker 1).
    it("queues a connecting port rather than dropping it", () => {
      const registry = new PortRegistry();
      const port = fakePort();

      registry.connect(port);

      expect(port.postMessage).not.toHaveBeenCalled();
      expect(port.onmessage).toBeNull();
    });

    it("delivers the queued port's ready handshake once the core activates", () => {
      const registry = new PortRegistry();
      const port = fakePort();
      registry.connect(port);

      registry.activate(vi.fn().mockResolvedValue(undefined), () => 7);

      expect(port.postMessage).toHaveBeenCalledWith({ type: "ready", apiVersion: 7 });
      expect(port.start).toHaveBeenCalled();
    });

    it("wires every port that connected while init was in flight, in order", () => {
      const registry = new PortRegistry();
      const first = fakePort();
      const second = fakePort();
      registry.connect(first);
      registry.connect(second);

      registry.activate(vi.fn().mockResolvedValue(undefined), () => 1);

      expect(first.postMessage).toHaveBeenCalledWith({ type: "ready", apiVersion: 1 });
      expect(second.postMessage).toHaveBeenCalledWith({ type: "ready", apiVersion: 1 });
    });

    it("routes a queued port's requests through the queue supplied at activate", () => {
      const enqueue = vi.fn().mockResolvedValue(undefined);
      const registry = new PortRegistry();
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
      const registry = new PortRegistry();
      const port = fakePort();
      registry.connect(port);

      registry.activateError("wasm init failed");

      expect(port.postMessage).toHaveBeenCalledWith({
        type: "error",
        message: "wasm init failed",
      });
    });

    it("posts error immediately to a port connecting after the failure", () => {
      const registry = new PortRegistry();
      registry.activateError("wasm init failed");

      const port = fakePort();
      registry.connect(port);

      expect(port.postMessage).toHaveBeenCalledWith({
        type: "error",
        message: "wasm init failed",
      });
    });
  });
});
