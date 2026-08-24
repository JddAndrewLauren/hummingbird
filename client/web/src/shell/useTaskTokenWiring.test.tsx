// @vitest-environment jsdom

import { renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { coreStore, type CoreStatus } from "../store/store";
import type { WorkerLike } from "../store/worker-client";
import { act } from "../test/component";
import type { TaskTokenRecord, TaskTokenStoreLike } from "../task/token-store";
import { useTaskTokenWiring } from "./useTaskTokenWiring";

// Mounted (via renderHook) rather than called, for
// `useLedgerWiring.test.tsx`'s own reason: a hook that is exported,
// unit-tested and never wired compiles clean and does nothing.
// `token.test.ts` already pins `loadTaskToken`/`submitTaskToken`/
// `forgetTaskToken`'s own logic against a fake `TaskTokenStoreLike`; this
// file pins only the hook's own wiring — which worker messages fire, and
// what the returned `hasToken`/`enteredAtMs` state settles to — reusing that
// same fake.

function fakeWorker(): WorkerLike & { postMessage: ReturnType<typeof vi.fn> } {
  return { onmessage: null, postMessage: vi.fn() };
}

function fakeStore(initial: TaskTokenRecord | null = null): TaskTokenStoreLike {
  let record = initial;
  return {
    read: () => Promise.resolve(record),
    write: (next) => {
      record = next;
      return Promise.resolve();
    },
    clear: () => {
      record = null;
      return Promise.resolve();
    },
  };
}

function types(worker: ReturnType<typeof fakeWorker>): string[] {
  return worker.postMessage.mock.calls.map(([message]) => (message as { type: string }).type);
}

function mount(
  worker: WorkerLike,
  status: CoreStatus,
  store: TaskTokenStoreLike,
  now: () => number = Date.now,
) {
  return renderHook(() => useTaskTokenWiring(worker, status, store, now));
}

describe("useTaskTokenWiring", () => {
  beforeEach(() => {
    coreStore.setTaskState({ needsReconnect: true });
  });

  it("asks nothing and reports no token while the core is still loading", async () => {
    const worker = fakeWorker();
    const { result } = mount(worker, "loading", fakeStore(null));
    await act(async () => {});
    expect(worker.postMessage).not.toHaveBeenCalled();
    expect(result.current).toEqual({
      hasToken: false,
      enteredAtMs: null,
      handleSubmitToken: expect.any(Function),
      handleForgetToken: expect.any(Function),
    });
  });

  it("a never-entered device stays tokenless once ready, without pushing anything to the worker", async () => {
    const worker = fakeWorker();
    const { result } = mount(worker, "ready", fakeStore(null));
    await act(async () => {});
    expect(worker.postMessage).not.toHaveBeenCalled();
    expect(result.current.hasToken).toBe(false);
    expect(result.current.enteredAtMs).toBeNull();
  });

  it("rehydrates a stored token via initTaskApiKey (never pushTaskApiKey) once the core is ready", async () => {
    const worker = fakeWorker();
    const { result } = mount(
      worker,
      "ready",
      fakeStore({ token: "secret-token", enteredAtMs: 1_000 }),
    );
    await act(async () => {});
    expect(types(worker)).toEqual(["initTaskApiKey"]);
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "initTaskApiKey", apiKey: "secret-token" });
    expect(result.current.hasToken).toBe(true);
    expect(result.current.enteredAtMs).toBe(1_000);
  });

  it("handleSubmitToken writes the store, pushes the token via pushTaskApiKey, and reports it resting", async () => {
    const worker = fakeWorker();
    const store = fakeStore(null);
    const { result } = mount(worker, "ready", store, () => 5_000);
    await act(async () => {});
    worker.postMessage.mockClear();

    let outcome: string | undefined;
    await act(async () => {
      outcome = await result.current.handleSubmitToken("fresh-token");
    });

    expect(outcome).toBe("ok");
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "pushTaskApiKey", apiKey: "fresh-token" });
    expect(result.current.hasToken).toBe(true);
    expect(result.current.enteredAtMs).toBe(5_000);
    await expect(store.read()).resolves.toEqual({ token: "fresh-token", enteredAtMs: 5_000 });
    expect(coreStore.getSnapshot().task.needsReconnect).toBe(false);
  });

  it("handleSubmitToken rejects a blank entry without touching the store or the worker", async () => {
    const worker = fakeWorker();
    const { result } = mount(worker, "ready", fakeStore(null));
    await act(async () => {});
    worker.postMessage.mockClear();

    let outcome: string | undefined;
    await act(async () => {
      outcome = await result.current.handleSubmitToken("   ");
    });

    expect(outcome).toBe("blank");
    expect(worker.postMessage).not.toHaveBeenCalled();
    expect(result.current.hasToken).toBe(false);
    expect(coreStore.getSnapshot().task.needsReconnect).toBe(true);
  });

  it("handleForgetToken clears the store and the live key via clearTaskApiKey", async () => {
    const worker = fakeWorker();
    const store = fakeStore({ token: "secret-token", enteredAtMs: 1_000 });
    const { result } = mount(worker, "ready", store);
    await act(async () => {});
    worker.postMessage.mockClear();

    await act(async () => {
      await result.current.handleForgetToken();
    });

    expect(worker.postMessage).toHaveBeenCalledWith({ type: "clearTaskApiKey" });
    expect(result.current.hasToken).toBe(false);
    expect(result.current.enteredAtMs).toBeNull();
    await expect(store.read()).resolves.toBeNull();
  });
});
