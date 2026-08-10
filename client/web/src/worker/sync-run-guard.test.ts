import { describe, expect, it, vi } from "vitest";
import { createSyncRunGuard } from "./sync-run-guard";

/** A controllable fake `runSync`: resolves only when the test calls the
 * returned `resolve`, so a test can assert what happens strictly *during*
 * an in-flight run before letting it finish. */
function deferredRunSync() {
  const calls: string[] = [];
  const resolvers: Array<() => void> = [];
  const runSync = vi.fn((trigger: string) => {
    calls.push(trigger);
    return new Promise<void>((resolve) => {
      resolvers.push(resolve);
    });
  });
  return {
    runSync,
    calls,
    resolveNext: () => resolvers.shift()?.(),
  };
}

describe("createSyncRunGuard", () => {
  it("starts a run immediately when nothing is in flight", () => {
    const { runSync, calls } = deferredRunSync();
    const guarded = createSyncRunGuard(runSync, { releaseMs: 30_000 });

    guarded("open");

    expect(calls).toEqual(["open"]);
  });

  it("does not start a second concurrent run while one is in flight", () => {
    const { runSync, calls } = deferredRunSync();
    const guarded = createSyncRunGuard(runSync, { releaseMs: 30_000 });

    guarded("open");
    guarded("focus");

    expect(calls).toEqual(["open"]);
  });

  it("coalesces any number of in-flight triggers into exactly one follow-up run", async () => {
    const { runSync, calls, resolveNext } = deferredRunSync();
    const guarded = createSyncRunGuard(runSync, { releaseMs: 30_000 });

    guarded("open");
    guarded("focus");
    guarded("reconnect");
    guarded("timer");

    expect(calls).toEqual(["open"]);

    resolveNext();
    // let the guard's own .then/.finally microtasks run
    await Promise.resolve();
    await Promise.resolve();

    expect(calls).toEqual(["open", "timer"]);

    resolveNext();
    await Promise.resolve();
    await Promise.resolve();

    // no further pending trigger, so no third run
    expect(calls).toEqual(["open", "timer"]);
  });

  it("defaults to last-wins when no mergePending is supplied", async () => {
    const { runSync, calls, resolveNext } = deferredRunSync();
    const guarded = createSyncRunGuard(runSync, { releaseMs: 30_000 });

    guarded("open");
    guarded("focus");
    guarded("timer");

    resolveNext();
    await Promise.resolve();
    await Promise.resolve();

    // no custom precedence: the last trigger to arrive is what runs next,
    // even though "open" arrived first.
    expect(calls).toEqual(["open", "timer"]);
  });

  it("uses a caller-supplied mergePending to decide the pending slot instead of last-wins", async () => {
    const { runSync, calls, resolveNext } = deferredRunSync();
    // A stand-in precedence: "open" always beats any other trigger.
    const mergePending = vi.fn((pending: string, incoming: string) =>
      pending === "open" || incoming === "open" ? "open" : incoming,
    );
    const guarded = createSyncRunGuard(runSync, { releaseMs: 30_000, mergePending });

    guarded("open");
    guarded("focus");
    guarded("open");
    guarded("timer");

    expect(calls).toEqual(["open"]);
    // mergePending is only consulted once a pending slot already holds a
    // trigger — "focus" was the first to land in the empty slot, so it is
    // the first call's `pending` argument, not the initial "open" itself
    // (that "open" started immediately and was never pending).
    expect(mergePending).toHaveBeenNthCalledWith(1, "focus", "open");
    expect(mergePending).toHaveBeenNthCalledWith(2, "open", "timer");

    resolveNext();
    await Promise.resolve();
    await Promise.resolve();

    // the higher-priority pending "open" survived being merged against the
    // later "timer", so it — not "timer" — is what runs next.
    expect(calls).toEqual(["open", "open"]);
  });

  it("does not call mergePending when the pending slot is empty — the first trigger to arrive while in flight just takes the slot", () => {
    const { runSync, calls } = deferredRunSync();
    const mergePending = vi.fn((_pending: string, incoming: string) => incoming);
    const guarded = createSyncRunGuard(runSync, { releaseMs: 30_000, mergePending });

    guarded("open");
    guarded("focus");

    expect(calls).toEqual(["open"]);
    expect(mergePending).not.toHaveBeenCalled();
  });

  it("starts a new run immediately, with no added latency, once the in-flight one resolves and nothing was pending", async () => {
    const { runSync, calls, resolveNext } = deferredRunSync();
    const guarded = createSyncRunGuard(runSync, { releaseMs: 30_000 });

    guarded("open");
    resolveNext();
    await Promise.resolve();
    await Promise.resolve();

    guarded("focus");

    expect(calls).toEqual(["open", "focus"]);
  });

  it("releases the guard after the release bound even if the run's promise never settles, and a pending trigger starts a new run at the bound", () => {
    vi.useFakeTimers();
    try {
      const runSync = vi.fn(() => new Promise<void>(() => {}));
      const guarded = createSyncRunGuard(runSync, { releaseMs: 30_000 });

      guarded("open");
      expect(runSync).toHaveBeenCalledTimes(1);

      // still within the bound: no release yet
      vi.advanceTimersByTime(29_999);
      guarded("focus");
      expect(runSync).toHaveBeenCalledTimes(1);

      // crosses the release bound: the guard frees the slot and, since a
      // trigger arrived while it was wedged, starts it immediately.
      vi.advanceTimersByTime(2);

      expect(runSync).toHaveBeenCalledTimes(2);
      expect(runSync).toHaveBeenLastCalledWith("focus");
    } finally {
      vi.useRealTimers();
    }
  });

  it("a trigger arriving after the release bound (with nothing pending) starts a new run immediately", () => {
    vi.useFakeTimers();
    try {
      const runSync = vi.fn(() => new Promise<void>(() => {}));
      const guarded = createSyncRunGuard(runSync, { releaseMs: 30_000 });

      guarded("open");
      vi.advanceTimersByTime(30_000);
      expect(runSync).toHaveBeenCalledTimes(1);

      guarded("timer");
      expect(runSync).toHaveBeenCalledTimes(2);
      expect(runSync).toHaveBeenLastCalledWith("timer");
    } finally {
      vi.useRealTimers();
    }
  });

  it("a straggling run that finally settles after the release bound does not corrupt the run started in its place", async () => {
    vi.useFakeTimers();
    try {
      const firstResolvers: Array<() => void> = [];
      let callCount = 0;
      const runSync = vi.fn((_trigger: string) => {
        callCount += 1;
        if (callCount === 1) {
          return new Promise<void>((resolve) => {
            firstResolvers.push(resolve);
          });
        }
        return Promise.resolve();
      });
      const guarded = createSyncRunGuard(runSync, { releaseMs: 30_000 });

      guarded("open");
      vi.advanceTimersByTime(30_000);
      // release bound crossed: guard is free again
      guarded("focus");
      expect(runSync).toHaveBeenCalledTimes(2);

      // the straggling first run now finally resolves
      firstResolvers[0]?.();
      await vi.advanceTimersByTimeAsync(0);

      // a trigger right after must still start a fresh run, proving the
      // guard's slot was not left wedged "in flight" by the straggler's
      // late resolution.
      guarded("timer");
      expect(runSync).toHaveBeenCalledTimes(3);
    } finally {
      vi.useRealTimers();
    }
  });
});
