import { describe, expect, it } from "vitest";
import {
  createDiagnosticsSession,
  networkChangedEvent,
  requestAbandonedEvent,
  requestBusyEvent,
  requestDequeuedEvent,
  requestEnqueuedEvent,
} from "./diagnostics-events";

describe("createDiagnosticsSession", () => {
  it("mints a stable id and increases seq monotonically across calls", () => {
    const session = createDiagnosticsSession(1_000);
    expect(session.nextSeq()).toBe(1);
    expect(session.nextSeq()).toBe(2);
    expect(session.nextSeq()).toBe(3);
  });

  it("computes elapsed time from its own origin, never a second clock", () => {
    const session = createDiagnosticsSession(1_000);
    expect(session.elapsedMs(1_500)).toBe(500);
    expect(session.elapsedMs(1_000)).toBe(0);
  });

  it("never returns a negative elapsed time for a nowMs before the origin", () => {
    const session = createDiagnosticsSession(1_000);
    expect(session.elapsedMs(500)).toBe(0);
  });

  it("falls back to a Math.random-derived id when the environment has no crypto.randomUUID", () => {
    const session = createDiagnosticsSession(0, undefined);
    expect(session.id.length).toBeGreaterThan(0);
  });
});

describe("event builders", () => {
  const session = createDiagnosticsSession(0);

  it("builds every worker-layer event with the closed contract's envelope shape", () => {
    const event = requestEnqueuedEvent(session, 1_000);
    expect(event).toMatchObject({
      schema_version: 1,
      source: "web-worker",
      cycle_id: null,
      operation_id: null,
      request_id: null,
      wall_clock_ms: 1_000,
    });
    expect(event.event).toEqual({ name: "core.wait_started" });
  });

  it("maps enqueue/dequeue/abandon/busy to their own distinct closed-enum names", () => {
    expect(requestEnqueuedEvent(session, 1_000).event.name).toBe("core.wait_started");
    expect(requestDequeuedEvent(session, 1_000).event.name).toBe("core.acquired");
    expect(requestAbandonedEvent(session, 1_000).event.name).toBe("operation.abandoned");
    expect(requestBusyEvent(session, 1_000).event.name).toBe("core.busy");
  });

  it("carries the online boolean on a network.changed payload", () => {
    expect(networkChangedEvent(session, 1_000, true).event).toEqual({
      name: "network.changed",
      payload: { online: true },
    });
    expect(networkChangedEvent(session, 1_000, false).event).toEqual({
      name: "network.changed",
      payload: { online: false },
    });
  });
});
