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
import type { BackendEntry } from "../skills/backend-registry";
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
  selectedItemId = "item-1",
  selection = "auto",
  registry,
  onSelectBackend,
}: {
  worker: WorkerLike;
  fetchImpl: typeof globalThis.fetch;
  store: TaskTokenStoreLike;
  selectedItemId?: string;
  selection?: string;
  registry?: BackendEntry[];
  onSelectBackend?: (id: string) => void;
}) {
  const { run, onRun, declinedFallback } = useMicrotaskWiring(worker, selectedItemId, selection, {
    fetch: fetchImpl,
    tokenStore: store,
    registry,
    onSelectBackend,
  });
  return (
    <>
      <span data-testid="phase">{run.phase}</span>
      {declinedFallback ? (
        <button type="button" onClick={() => declinedFallback.onSwitchAndRun({ itemId: "item-1" })}>
          switch to {declinedFallback.label}
        </button>
      ) : null}
      {["item-1", "item-2"].map((itemId) => (
        <button key={itemId} type="button" aria-label={itemId} onClick={() => onRun({ itemId })}>
          run
        </button>
      ))}
    </>
  );
}

function phase(): string {
  return screen.getByTestId("phase").textContent ?? "";
}

function tap(itemId = "item-1"): void {
  fireEvent.click(screen.getByLabelText(itemId));
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

    tap();
    await settle();

    expect(fetchImpl).toHaveBeenCalledTimes(1);
    expect(fetchImpl.mock.calls[0]?.[0]).toBe("/api/skills/run");
    expect(phase()).toBe("done");
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

    tap();
    await settle();

    expect(phase()).toBe("declined");
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

    tap();
    await settle();
    expect(phase()).toBe("running");

    tap();
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

    tap();
    await settle();

    expect(fetchImpl).not.toHaveBeenCalled();
    expect(phase()).toBe("declined");
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

    tap();
    await settle();

    expect(phase()).toBe("declined");
    expect(worker.postMessage).not.toHaveBeenCalled();
  });

  /**
   * The regression this exists for: with one `AbortController` for the whole
   * hook, starting a run on item-2 aborted item-1's, and item-1 was left
   * reading "The run ended without an answer." — a lie, since the runner
   * writes to the authority and item-1's checklist was very likely landing
   * while the app said it had not. Runs on different items are independent.
   */
  it("a run on one item does not abort a run on another", async () => {
    const worker = fakeWorker();
    const releases = new Map<string, () => void>();
    const fetchImpl = vi.fn(async (_input: unknown, init?: RequestInit) => {
      const ref = JSON.parse(String(init?.body)).args.ref as string;
      const body = new ReadableStream<Uint8Array>({
        start(controller) {
          const encoder = new TextEncoder();
          controller.enqueue(encoder.encode('{"type":"progress","message":"reading"}\n'));
          // A real `fetch` errors the body stream when its signal aborts.
          // Honouring that here is what makes this test able to fail: a
          // fake that ignores the signal would pass whether or not the
          // controllers are per-item.
          init?.signal?.addEventListener("abort", () => {
            controller.error(new DOMException("The operation was aborted.", "AbortError"));
          });
          releases.set(ref, () => {
            controller.enqueue(
              encoder.encode('{"ok":true,"result":null,"backend":"anthropic","model":null}\n'),
            );
            controller.close();
          });
        },
      });
      return new Response(body, { status: 200 });
    });
    const { rerender } = render(
      <Harness worker={worker} fetchImpl={fetchImpl as never} store={tokenStore()} />,
    );

    tap("item-1");
    await settle();
    tap("item-2");
    await settle();
    expect(fetchImpl).toHaveBeenCalledTimes(2);

    // item-1's stream is still open — the second run did not tear it down.
    releases.get("item-1")?.();
    await settle();
    expect(phase()).toBe("done");

    // ...and item-2's is its own, still running until released.
    rerender(
      <Harness
        worker={worker}
        fetchImpl={fetchImpl as never}
        store={tokenStore()}
        selectedItemId="item-2"
      />,
    );
    expect(phase()).toBe("running");

    releases.get("item-2")?.();
    await settle();
    expect(phase()).toBe("done");
    // One cycle per completed run, neither lost to the other's abort.
    expect(manualSyncCount(worker)).toBe(2);
  });
});

/** #274's picker: which entry a run actually attempts, and the affordance
 * offered when a pin is dead. */
describe("useMicrotaskWiring — #274's routing", () => {
  const CLOUD: BackendEntry = { id: "cloud", label: "Cloud runner", model: null, endpoint: "/api/skills/run", connectTimeoutMs: 2500 };
  const HOME: BackendEntry = { id: "home", label: "Home runner", model: "llama3", endpoint: "/api/home-run", connectTimeoutMs: 2500 };

  it("Auto attempts the first registered entry's own endpoint", async () => {
    const worker = fakeWorker();
    const fetchImpl = vi.fn(async (_input: unknown) =>
      ndjson('{"ok":true,"result":null,"backend":"anthropic","model":"opus"}'),
    );
    render(
      <Harness
        worker={worker}
        fetchImpl={fetchImpl as never}
        store={tokenStore()}
        selection="auto"
        registry={[CLOUD, HOME]}
      />,
    );

    tap();
    await settle();

    expect(fetchImpl.mock.calls[0]?.[0]).toBe("/api/skills/run");
    expect(phase()).toBe("done");
  });

  it("a pin attempts only its own entry, using its endpoint", async () => {
    const worker = fakeWorker();
    const fetchImpl = vi.fn(async (_input: unknown) =>
      ndjson('{"ok":true,"result":null,"backend":"home-runner","model":"llama3"}'),
    );
    render(
      <Harness
        worker={worker}
        fetchImpl={fetchImpl as never}
        store={tokenStore()}
        selection="home"
        registry={[CLOUD, HOME]}
      />,
    );

    tap();
    await settle();

    expect(fetchImpl.mock.calls[0]?.[0]).toBe("/api/home-run");
  });

  it("a dead pin's one-tap switch retries against the NEW backend, not the pin it just left", async () => {
    // The bug this pins: switching and re-running used to be two calls in
    // one tick, and `onRun` is closed over the render's `selection` — so
    // the retry re-attempted the dead pin (and, freshly memoized dead,
    // declined instantly without sending anything). The selection prop here
    // deliberately stays "cloud" for the whole test, exactly as it does in
    // the tick before React re-renders with the new preference.
    const worker = fakeWorker();
    const fetchImpl = vi.fn(async (input: RequestInfo | URL) =>
      String(input) === "/api/home-run"
        ? ndjson('{"ok":true,"result":null,"backend":"home","model":"llama3"}')
        : Promise.reject(new Error("connection refused")),
    );
    const onSelectBackend = vi.fn();
    render(
      <Harness
        worker={worker}
        fetchImpl={fetchImpl as never}
        store={tokenStore()}
        selection="cloud"
        registry={[CLOUD, HOME]}
        onSelectBackend={onSelectBackend}
      />,
    );

    tap();
    await settle();

    expect(phase()).toBe("declined");
    expect(fetchImpl.mock.calls.map((call) => String(call[0]))).toEqual(["/api/skills/run"]);

    fireEvent.click(screen.getByText(/switch to home runner/i));
    await settle();

    // Both halves, and the second is the one that used to be wrong: the
    // preference moved, AND the retry went to home.
    expect(onSelectBackend).toHaveBeenCalledWith("home");
    expect(fetchImpl.mock.calls.map((call) => String(call[0]))).toEqual([
      "/api/skills/run",
      "/api/home-run",
    ]);
    expect(phase()).toBe("done");
  });

  it("offers no fallback while pinned to the registry's only entry", async () => {
    const worker = fakeWorker();
    const fetchImpl = vi.fn(async () => {
      throw new Error("connection refused");
    });
    render(
      <Harness
        worker={worker}
        fetchImpl={fetchImpl as never}
        store={tokenStore()}
        selection="cloud"
        registry={[CLOUD]}
        onSelectBackend={vi.fn()}
      />,
    );

    tap();
    await settle();

    expect(phase()).toBe("declined");
    expect(screen.queryByText(/switch to/i)).toBeNull();
  });

  it("offers no fallback for a NO_TOKEN decline — the failure names nothing to switch away FROM", async () => {
    // NO_TOKEN is not evidence the pinned backend is unreachable (no
    // request was ever made), so "switch to Home runner" would be an
    // actively wrong suggestion here.
    const worker = fakeWorker();
    const fetchImpl = vi.fn();
    render(
      <Harness
        worker={worker}
        fetchImpl={fetchImpl as never}
        store={tokenStore(null)}
        selection="cloud"
        registry={[CLOUD, HOME]}
        onSelectBackend={vi.fn()}
      />,
    );

    tap();
    await settle();

    expect(fetchImpl).not.toHaveBeenCalled();
    expect(phase()).toBe("declined");
    expect(screen.queryByText(/switch to/i)).toBeNull();
  });

  it("offers no fallback under Auto — nothing to fall back FROM", async () => {
    const worker = fakeWorker();
    const fetchImpl = vi.fn(async () => {
      throw new Error("connection refused");
    });
    render(
      <Harness
        worker={worker}
        fetchImpl={fetchImpl as never}
        store={tokenStore()}
        selection="auto"
        registry={[CLOUD, HOME]}
        onSelectBackend={vi.fn()}
      />,
    );

    tap();
    await settle();

    expect(phase()).toBe("declined");
    expect(screen.queryByText(/switch to/i)).toBeNull();
  });
});
