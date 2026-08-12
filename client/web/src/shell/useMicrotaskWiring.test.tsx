// @vitest-environment jsdom
//
// The two behaviours that only exist once the hook is mounted: one tap
// issues exactly one request, and a terminal **ok** — and only a terminal
// ok — asks the shared cadence for one sync cycle. The second is #273's
// "the steps appear through the normal read path" in its entirety: nothing
// here reads steps, it just triggers the cycle `useItemDetailWiring`
// already re-reads on.

import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "../test/component";
import type { TaskTokenStoreLike } from "../task/token-store";
import type { WorkerLike } from "../store/worker-client";
import { useMicrotaskWiring } from "./useMicrotaskWiring";

function fakeWorker(): WorkerLike & { postMessage: ReturnType<typeof vi.fn> } {
  return { onmessage: null, postMessage: vi.fn() };
}

function tokenStore(token: string | null = "hb_device_token"): TaskTokenStoreLike {
  return {
    read: async () => (token === null ? null : { token, enteredAtMs: 1_000 }),
    write: async () => {},
    clear: async () => {},
  };
}

function ndjson(...lines: string[]): Response {
  const body = new ReadableStream<Uint8Array>({
    start(controller) {
      const encoder = new TextEncoder();
      for (const line of lines) controller.enqueue(encoder.encode(`${line}\n`));
      controller.close();
    },
  });
  return new Response(body, { status: 200 });
}

function Harness({
  worker,
  fetchImpl,
  store,
}: {
  worker: WorkerLike;
  fetchImpl: typeof globalThis.fetch;
  store: TaskTokenStoreLike;
}) {
  const { run, onRun } = useMicrotaskWiring(worker, "item-1", {
    fetch: fetchImpl,
    tokenStore: store,
  });
  return (
    <button type="button" onClick={() => onRun({ itemId: "item-1" })}>
      {run.phase}
    </button>
  );
}

async function settle(): Promise<void> {
  // Two macrotask turns: enough for the stream reader's microtasks and the
  // state updates they schedule.
  await new Promise((resolve) => setTimeout(resolve, 0));
  await new Promise((resolve) => setTimeout(resolve, 0));
}

function manualSyncCount(worker: ReturnType<typeof fakeWorker>): number {
  return worker.postMessage.mock.calls.filter(([m]) => m.type === "manualSyncTrigger").length;
}

describe("useMicrotaskWiring", () => {
  it("one tap issues exactly one request, and a terminal ok asks for one cycle", async () => {
    const worker = fakeWorker();
    const fetchImpl = vi.fn(async (_input: unknown) =>
      ndjson(
        '{"type":"progress","message":"reading"}',
        '{"ok":true,"skill":"microtask","result":{"steps":["a"],"note":""},"backend":"anthropic","model":"opus"}',
      ),
    );
    render(<Harness worker={worker} fetchImpl={fetchImpl as never} store={tokenStore()} />);

    fireEvent.click(screen.getByRole("button"));
    await settle();

    expect(fetchImpl).toHaveBeenCalledTimes(1);
    expect(fetchImpl.mock.calls[0]?.[0]).toBe("/api/skills/run");
    expect(screen.getByRole("button").textContent).toBe("done");
    expect(manualSyncCount(worker)).toBe(1);
  });

  /** #307 point 4 puts the drop after validation, so a decline leaves the
   * plan intact — there is nothing new to pull. */
  it("a decline fires no sync cycle at all", async () => {
    const worker = fakeWorker();
    const fetchImpl = vi.fn(async () =>
      ndjson('{"ok":false,"skill":"microtask","error":"already has a plan","backend":"anthropic","model":null}'),
    );
    render(<Harness worker={worker} fetchImpl={fetchImpl as never} store={tokenStore()} />);

    fireEvent.click(screen.getByRole("button"));
    await settle();

    expect(screen.getByRole("button").textContent).toBe("declined");
    expect(manualSyncCount(worker)).toBe(0);
  });

  it("a second tap while streaming starts no second request", async () => {
    const worker = fakeWorker();
    let release: () => void = () => {};
    const fetchImpl = vi.fn(async () => {
      const body = new ReadableStream<Uint8Array>({
        start(controller) {
          controller.enqueue(new TextEncoder().encode('{"type":"progress","message":"reading"}\n'));
          release = () => {
            controller.enqueue(
              new TextEncoder().encode('{"ok":true,"result":null,"backend":"a","model":null}\n'),
            );
            controller.close();
          };
        },
      });
      return new Response(body, { status: 200 });
    });
    render(<Harness worker={worker} fetchImpl={fetchImpl as never} store={tokenStore()} />);

    fireEvent.click(screen.getByRole("button"));
    await settle();
    expect(screen.getByRole("button").textContent).toBe("running");

    fireEvent.click(screen.getByRole("button"));
    await settle();
    expect(fetchImpl).toHaveBeenCalledTimes(1);

    release();
    await settle();
    expect(manualSyncCount(worker)).toBe(1);
  });

  it("with no token stored the run declines without a request, and nothing syncs", async () => {
    const worker = fakeWorker();
    const fetchImpl = vi.fn();
    render(<Harness worker={worker} fetchImpl={fetchImpl as never} store={tokenStore(null)} />);

    fireEvent.click(screen.getByRole("button"));
    await settle();

    expect(fetchImpl).not.toHaveBeenCalled();
    expect(screen.getByRole("button").textContent).toBe("declined");
    expect(manualSyncCount(worker)).toBe(0);
  });

  /** #273's "nothing is enqueued". The source pins in
   * `skills/no-queue.test.ts` prove the lane *cannot* reach the queue; this
   * proves that in a real run it in fact posts nothing but the one cadence
   * trigger. */
  it("a failed run posts nothing to the worker at all", async () => {
    const worker = fakeWorker();
    const fetchImpl = vi.fn(async () => {
      throw new Error("Failed to fetch");
    });
    render(<Harness worker={worker} fetchImpl={fetchImpl as never} store={tokenStore()} />);

    fireEvent.click(screen.getByRole("button"));
    await settle();

    expect(screen.getByRole("button").textContent).toBe("declined");
    expect(worker.postMessage).not.toHaveBeenCalled();
  });
});
