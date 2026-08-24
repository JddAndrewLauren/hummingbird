import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DiagnosticEventV1DTO } from "../store/protocol";
import type { DiagnosticsStoreLike } from "./diagnostics-store";
import { createDiagnosticsJournal } from "./diagnostics-journal";

function fakeStore(): DiagnosticsStoreLike & { appended: DiagnosticEventV1DTO[][] } {
  const appended: DiagnosticEventV1DTO[][] = [];
  return {
    appended,
    append: vi.fn(async (events: DiagnosticEventV1DTO[]) => {
      appended.push(events);
      return 0;
    }),
    exportAll: vi.fn(async () => ({ events: [], droppedCount: 0 })),
    clear: vi.fn(async () => {}),
  };
}

describe("createDiagnosticsJournal", () => {
  it("records an enqueue as a single core.wait_started, web-worker-sourced event", () => {
    const store = fakeStore();
    const journal = createDiagnosticsJournal(0, store);

    journal.recordEnqueued(1_000);

    expect(store.appended).toHaveLength(1);
    const [event] = store.appended[0];
    expect(event.event.name).toBe("core.wait_started");
    expect(event.source).toBe("web-worker");
    expect(event.wall_clock_ms).toBe(1_000);
  });

  it("records dequeue, abandon and busy as their own distinct event names", () => {
    const store = fakeStore();
    const journal = createDiagnosticsJournal(0, store);

    journal.recordDequeued(1_000);
    journal.recordAbandoned(1_000);
    journal.recordBusy(1_000);

    const names = store.appended.map((batch) => batch[0].event.name);
    expect(names).toEqual(["core.acquired", "operation.stalled", "core.busy"]);
  });

  it("records a network change with the online payload", () => {
    const store = fakeStore();
    const journal = createDiagnosticsJournal(0, store);

    journal.recordNetworkChanged(1_000, false);

    const event = store.appended[0][0];
    expect(event.event).toEqual({ name: "network.changed", payload: { online: false } });
  });

  it("assigns strictly increasing session seq across every recorded event", () => {
    const store = fakeStore();
    const journal = createDiagnosticsJournal(0, store);

    journal.recordEnqueued(1_000);
    journal.recordDequeued(1_001);
    journal.recordBusy(1_002);

    const seqs = store.appended.map((batch) => batch[0].seq);
    expect(seqs).toEqual([1, 2, 3]);
  });

  it("export delegates to the store", async () => {
    const store = fakeStore();
    const journal = createDiagnosticsJournal(0, store);

    await journal.export();

    expect(store.exportAll).toHaveBeenCalledTimes(1);
  });

  it("clear delegates to the store", async () => {
    const store = fakeStore();
    const journal = createDiagnosticsJournal(0, store);

    await journal.clear();

    expect(store.clear).toHaveBeenCalledTimes(1);
  });

  it("swallows a storage failure: recording still returns, and does not affect the caller", () => {
    const store: DiagnosticsStoreLike = {
      append: vi.fn(() => Promise.reject(new Error("quota exceeded"))),
      exportAll: vi.fn(() => Promise.reject(new Error("blocked upgrade"))),
      clear: vi.fn(() => Promise.reject(new Error("aborted"))),
    };
    const journal = createDiagnosticsJournal(0, store);

    // None of these may throw, synchronously or via an unhandled rejection
    // that reaches this test — a diagnostic write failure must never be
    // visible to capture, sync or startup.
    expect(() => journal.recordEnqueued(1_000)).not.toThrow();
    expect(() => journal.recordBusy(1_000)).not.toThrow();
    expect(() => journal.recordNetworkChanged(1_000, true)).not.toThrow();
  });

  it("still runs the wrapped request to completion when the store itself is broken", async () => {
    const store: DiagnosticsStoreLike = {
      append: vi.fn(() => Promise.reject(new Error("quota exceeded"))),
      exportAll: vi.fn(() => Promise.reject(new Error("x"))),
      clear: vi.fn(() => Promise.reject(new Error("x"))),
    };
    const journal = createDiagnosticsJournal(0, store);

    const result = await journal.drainAroundRequest(
      () => Promise.resolve("the real work completed"),
      () => JSON.stringify([{ name: "core.wait_started" }]),
      () => 1_000,
    );

    expect(result).toBe("the real work completed");
  });

  describe("drainAroundRequest", () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it("drains the host once before and once after a fast request, with nothing in between", async () => {
      const store = fakeStore();
      const journal = createDiagnosticsJournal(0, store);
      const drainHost = vi.fn(() => JSON.stringify([{ tag: "one-host-event" }]));

      await journal.drainAroundRequest(() => Promise.resolve("ok"), drainHost, () => 1_000);

      expect(drainHost).toHaveBeenCalledTimes(2);
    });

    it("drains again every 250ms while the request is still outstanding", async () => {
      const store = fakeStore();
      const journal = createDiagnosticsJournal(0, store);
      const drainHost = vi.fn(() => null);
      let resolveRun!: () => void;
      const run = new Promise<void>((resolve) => {
        resolveRun = resolve;
      });

      const pending = journal.drainAroundRequest(() => run, drainHost, () => 1_000);

      // Before: 1 call.
      expect(drainHost).toHaveBeenCalledTimes(1);

      await vi.advanceTimersByTimeAsync(250);
      expect(drainHost).toHaveBeenCalledTimes(2);
      await vi.advanceTimersByTimeAsync(250);
      expect(drainHost).toHaveBeenCalledTimes(3);

      resolveRun();
      await pending;
      // The trailing "after" drain, plus no further ticks once the timer
      // is cleared.
      expect(drainHost).toHaveBeenCalledTimes(4);
      await vi.advanceTimersByTimeAsync(1_000);
      expect(drainHost).toHaveBeenCalledTimes(4);
    });

    it("stores nothing when the host has nothing to drain, ever", async () => {
      const store = fakeStore();
      const journal = createDiagnosticsJournal(0, store);

      await journal.drainAroundRequest(() => Promise.resolve(), () => undefined, () => 1_000);

      expect(store.appended).toEqual([]);
    });

    it("swallows a malformed drain payload rather than throwing", async () => {
      const store = fakeStore();
      const journal = createDiagnosticsJournal(0, store);

      await expect(
        journal.drainAroundRequest(() => Promise.resolve(), () => "not json", () => 1_000),
      ).resolves.toBeUndefined();
      expect(store.appended).toEqual([]);
    });

    it("still clears its interval and drains once more when the wrapped request rejects", async () => {
      const store = fakeStore();
      const journal = createDiagnosticsJournal(0, store);
      const drainHost = vi.fn(() => null);

      await expect(
        journal.drainAroundRequest(() => Promise.reject(new Error("boom")), drainHost, () => 1_000),
      ).rejects.toThrow("boom");

      const callsAtSettle = drainHost.mock.calls.length;
      await vi.advanceTimersByTimeAsync(1_000);
      expect(drainHost).toHaveBeenCalledTimes(callsAtSettle);
    });
  });
});
