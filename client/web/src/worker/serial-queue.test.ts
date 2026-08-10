import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createSerialQueue } from "./serial-queue";

describe("createSerialQueue", () => {
  it("runs requests one at a time, in arrival order", async () => {
    const order: string[] = [];
    const queue = createSerialQueue<string>(
      async (request) => {
        order.push(`start:${request}`);
        await Promise.resolve();
        order.push(`end:${request}`);
      },
      { timeoutMs: 1_000, onTimeout: vi.fn() },
    );

    await Promise.all([queue("a"), queue("b")]);

    expect(order).toEqual(["start:a", "end:a", "start:b", "end:b"]);
  });

  it("keeps draining after a handler rejects, reporting it via onError", async () => {
    const onError = vi.fn();
    const queue = createSerialQueue<string>(
      async (request) => {
        if (request === "boom") {
          throw new Error("boom");
        }
      },
      { timeoutMs: 1_000, onTimeout: vi.fn(), onError },
    );

    await queue("boom");
    await queue("fine");

    expect(onError).toHaveBeenCalledWith("boom", expect.any(Error));
  });

  it("logs a rejected handler by default when no onError is supplied", async () => {
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
    const queue = createSerialQueue<string>(
      async () => {
        throw new Error("boom");
      },
      { timeoutMs: 1_000, onTimeout: vi.fn() },
    );

    await queue("x");

    expect(consoleError).toHaveBeenCalled();
    consoleError.mockRestore();
  });

  describe("a request that never settles", () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it("is abandoned after the timeout, and the queue moves on to the next request", async () => {
      const onTimeout = vi.fn();
      let secondRan = false;
      const queue = createSerialQueue<string>(
        async (request) => {
          if (request === "stuck") {
            await new Promise<void>(() => {
              // never resolves
            });
          } else {
            secondRan = true;
          }
        },
        { timeoutMs: 5_000, onTimeout },
      );

      const first = queue("stuck");
      const second = queue("second");

      // Before the timeout elapses, the second request must not have run
      // yet — the queue is still genuinely one-at-a-time up to that point.
      await vi.advanceTimersByTimeAsync(1_000);
      expect(secondRan).toBe(false);

      await vi.advanceTimersByTimeAsync(4_100);
      await first;
      await second;

      expect(onTimeout).toHaveBeenCalledWith("stuck");
      expect(secondRan).toBe(true);
    });

    it("does not double-resolve if the abandoned handler eventually settles", async () => {
      const onTimeout = vi.fn();
      let resolveStuck!: () => void;
      const queue = createSerialQueue<string>(
        async (request) => {
          if (request === "stuck") {
            await new Promise<void>((resolve) => {
              resolveStuck = resolve;
            });
          }
        },
        { timeoutMs: 1_000, onTimeout },
      );

      const first = queue("stuck");
      await vi.advanceTimersByTimeAsync(1_100);
      await first;
      expect(onTimeout).toHaveBeenCalledTimes(1);

      // The straggler finally resolves after the queue already moved on —
      // must not throw or re-trigger anything observable.
      resolveStuck();
      await vi.advanceTimersByTimeAsync(0);
      expect(onTimeout).toHaveBeenCalledTimes(1);
    });
  });
});
