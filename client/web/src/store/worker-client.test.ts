import { describe, expect, it, vi } from "vitest";
import { createCoreStore } from "./store";
import { attachWorkerClient } from "./worker-client";

// A minimal fake of the Worker surface the client needs: postMessage +
// an assignable onmessage handler the test can drive directly.
function fakeWorker() {
  return {
    postMessage: vi.fn(),
    onmessage: null as ((event: MessageEvent) => void) | null,
  };
}

describe("attachWorkerClient", () => {
  it("posts an init request to the worker on attach", () => {
    const worker = fakeWorker();
    const store = createCoreStore();

    attachWorkerClient(worker, store);

    expect(worker.postMessage).toHaveBeenCalledWith({ type: "init" });
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
