import { describe, expect, it } from "vitest";
import { createCoreStore } from "./store";
import { attachWorkerClient } from "./worker-client";

// A minimal fake of the Worker surface the client needs: just an assignable
// onmessage handler the test can drive directly. Deliberately NO postMessage:
// the client must never send anything (PR #79 round-2 blocker — a request
// posted at construction races the worker's async module evaluation and is
// dropped; the worker announces readiness itself instead).
function fakeWorker() {
  return {
    onmessage: null as ((event: MessageEvent) => void) | null,
  };
}

describe("attachWorkerClient", () => {
  it("only listens on attach — sends nothing, so there is no init race to lose", () => {
    const worker = fakeWorker();
    const store = createCoreStore();

    attachWorkerClient(worker, store);

    expect(worker.onmessage).toBeTypeOf("function");
    expect(store.getSnapshot().status).toBe("loading");
  });

  it("moves the store to ready with the reported api version on a ready message", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({ data: { type: "ready", apiVersion: 3 } } as MessageEvent);

    expect(store.getSnapshot()).toEqual({
      status: "ready",
      apiVersion: 3,
      error: null,
    });
  });

  it("moves the store to error with the reported message on an error message", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: { type: "error", message: "wasm init failed" },
    } as MessageEvent);

    expect(store.getSnapshot()).toEqual({
      status: "error",
      apiVersion: null,
      error: "wasm init failed",
    });
  });
});
