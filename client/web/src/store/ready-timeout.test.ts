import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createCoreStore } from "./store";
import { watchForReadyTimeout } from "./ready-timeout";

// PR #167 round-1 review, blocker 2: a SharedWorker's own `onerror` only
// covers the initial script *fetch* failing, not an uncaught error during
// its script's evaluation (e.g. a CSP rejecting WebAssembly compilation) —
// the exact case main.tsx's comment used to (wrongly) claim `onerror`
// covered. `core.worker.ts`'s `PortRegistry.activateError` path is the
// primary fix (it posts a real `{type: "error"}` message the existing
// `attachWorkerClient` already routes to the store), but this is the
// view-side backstop for anything that reaches neither: the core simply
// never reports in.

describe("watchForReadyTimeout", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("moves the store to error if still loading once the timeout elapses", () => {
    const store = createCoreStore();

    watchForReadyTimeout(store, 5_000);
    vi.advanceTimersByTime(5_000);

    expect(store.getSnapshot().status).toBe("error");
    expect(store.getSnapshot().error).toMatch(/did not report ready/i);
  });

  it("does nothing once the timeout elapses if the core already reported ready", () => {
    const store = createCoreStore();
    store.setState({ status: "ready", apiVersion: 1 });

    watchForReadyTimeout(store, 5_000);
    vi.advanceTimersByTime(5_000);

    expect(store.getSnapshot()).toEqual({
      status: "ready",
      apiVersion: 1,
      coreId: null,
      viewOrdinal: null,
      error: null,
      calendar: store.getSnapshot().calendar,
      task: store.getSnapshot().task,
    });
  });

  it("does nothing once the timeout elapses if the core already reported its own error", () => {
    const store = createCoreStore();
    store.setState({ status: "error", error: "wasm init failed" });

    watchForReadyTimeout(store, 5_000);
    vi.advanceTimersByTime(5_000);

    // The core's own, more specific error message must survive — the
    // timeout is a backstop, not a message that should ever overwrite one
    // that already arrived.
    expect(store.getSnapshot().error).toBe("wasm init failed");
  });
});
