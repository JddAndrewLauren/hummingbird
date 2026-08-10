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
  it("announces ready to a newly connecting port", () => {
    const enqueue = vi.fn().mockResolvedValue(undefined);
    const registry = new PortRegistry(enqueue, () => 3);
    const port = fakePort();

    registry.connect(port);

    expect(port.postMessage).toHaveBeenCalledWith({ type: "ready", apiVersion: 3 });
  });

  it("broadcasts a published event to every connected port", () => {
    const enqueue = vi.fn().mockResolvedValue(undefined);
    const registry = new PortRegistry(enqueue, () => 1);
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
    const enqueue = vi.fn().mockResolvedValue(undefined);
    const registry = new PortRegistry(enqueue, () => 5);
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
    const registry = new PortRegistry(enqueue, () => 1);
    const port = fakePort();
    registry.connect(port);

    const request: CalendarWorkerRequest = { type: "listCalendars" };
    port.onmessage?.({ data: request } as MessageEvent<CalendarWorkerRequest>);

    expect(enqueue).toHaveBeenCalledWith(request);
  });

  it("starts a connecting port so its queued incoming messages dispatch", () => {
    const enqueue = vi.fn().mockResolvedValue(undefined);
    const registry = new PortRegistry(enqueue, () => 1);
    const port = fakePort();

    registry.connect(port);

    expect(port.start).toHaveBeenCalled();
  });
});
