// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import type { CoreStatus } from "../store/store";
import type { WorkerLike } from "../store/worker-client";
import { act, render } from "../test/component";
import type { TaskTokenRecord, TaskTokenStoreLike } from "../task/token-store";
import { useTaskTokenWiring, type TaskTokenWiring } from "./useTaskTokenWiring";

// Mounted rather than called, for `useLedgerWiring.test.tsx`'s own reason: a
// hook that is exported, unit-tested and never wired compiles clean and does
// nothing. `token.test.ts` already pins `loadTaskToken`/`submitTaskToken`/
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

let latest: TaskTokenWiring | null = null;

function Probe({
  worker,
  status,
  store,
  now = Date.now,
}: {
  worker: WorkerLike;
  status: CoreStatus;
  store: TaskTokenStoreLike;
  now?: () => number;
}) {
  latest = useTaskTokenWiring(worker, status, store, now);
  return null;
}

describe("useTaskTokenWiring", () => {
  it("asks nothing and reports no token while the core is still loading", async () => {
    const worker = fakeWorker();
    render(<Probe worker={worker} status="loading" store={fakeStore(null)} />);
    await act(async () => {});
    expect(worker.postMessage).not.toHaveBeenCalled();
    expect(latest).toEqual({
      hasToken: false,
      enteredAtMs: null,
      handleSubmitToken: expect.any(Function),
      handleForgetToken: expect.any(Function),
    });
  });

  it("a never-entered device stays tokenless once ready, without pushing anything to the worker", async () => {
    const worker = fakeWorker();
    render(<Probe worker={worker} status="ready" store={fakeStore(null)} />);
    await act(async () => {});
    expect(worker.postMessage).not.toHaveBeenCalled();
    expect(latest!.hasToken).toBe(false);
    expect(latest!.enteredAtMs).toBeNull();
  });

  it("rehydrates a stored token via initTaskApiKey (never pushTaskApiKey) once the core is ready", async () => {
    const worker = fakeWorker();
    render(
      <Probe
        worker={worker}
        status="ready"
        store={fakeStore({ token: "secret-token", enteredAtMs: 1_000 })}
      />,
    );
    await act(async () => {});
    expect(types(worker)).toEqual(["initTaskApiKey"]);
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "initTaskApiKey", apiKey: "secret-token" });
    expect(latest!.hasToken).toBe(true);
    expect(latest!.enteredAtMs).toBe(1_000);
  });

  it("handleSubmitToken writes the store, pushes the token via pushTaskApiKey, and reports it resting", async () => {
    const worker = fakeWorker();
    const store = fakeStore(null);
    render(<Probe worker={worker} status="ready" store={store} now={() => 5_000} />);
    await act(async () => {});
    worker.postMessage.mockClear();

    let outcome: string | undefined;
    await act(async () => {
      outcome = await latest!.handleSubmitToken("fresh-token");
    });

    expect(outcome).toBe("ok");
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "pushTaskApiKey", apiKey: "fresh-token" });
    expect(latest!.hasToken).toBe(true);
    expect(latest!.enteredAtMs).toBe(5_000);
    await expect(store.read()).resolves.toEqual({ token: "fresh-token", enteredAtMs: 5_000 });
  });

  it("handleSubmitToken rejects a blank entry without touching the store or the worker", async () => {
    const worker = fakeWorker();
    render(<Probe worker={worker} status="ready" store={fakeStore(null)} />);
    await act(async () => {});
    worker.postMessage.mockClear();

    let outcome: string | undefined;
    await act(async () => {
      outcome = await latest!.handleSubmitToken("   ");
    });

    expect(outcome).toBe("blank");
    expect(worker.postMessage).not.toHaveBeenCalled();
    expect(latest!.hasToken).toBe(false);
  });

  it("handleForgetToken clears the store and the live key via clearTaskApiKey", async () => {
    const worker = fakeWorker();
    const store = fakeStore({ token: "secret-token", enteredAtMs: 1_000 });
    render(<Probe worker={worker} status="ready" store={store} />);
    await act(async () => {});
    worker.postMessage.mockClear();

    await act(async () => {
      await latest!.handleForgetToken();
    });

    expect(worker.postMessage).toHaveBeenCalledWith({ type: "clearTaskApiKey" });
    expect(latest!.hasToken).toBe(false);
    expect(latest!.enteredAtMs).toBeNull();
    await expect(store.read()).resolves.toBeNull();
  });
});
