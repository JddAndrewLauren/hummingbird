// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "../test/component";
import type { WorkerLike } from "../store/worker-client";
import type { StepDTO } from "../store/protocol";
import { useGrillTakeoverWiring } from "./useGrillTakeoverWiring";

function fakeWorker(): WorkerLike & { postMessage: ReturnType<typeof vi.fn> } {
  return { onmessage: null, postMessage: vi.fn() };
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

const QUESTION_LINE =
  '{"ok":true,"skill":"grill-me","result":{"kind":"question","question":{"prompt":"Which airport?","recommendedAnswer":"SEA","choices":["SEA","PDX"]}},"backend":"cloud","model":"opus"}';
const PROPOSAL_LINE =
  '{"ok":true,"skill":"grill-me","result":{"kind":"proposal","proposal":{"summary":"s","verdict":"resolved","patch":{}}},"backend":"cloud","model":"opus"}';

function step(overrides: Partial<StepDTO> = {}): StepDTO {
  return {
    id: "step-1",
    itemId: "item-1",
    body: "pack",
    done: false,
    position: 0,
    deletedAt: null,
    version: 1,
    ...overrides,
  };
}

function Harness({
  worker,
  fetchImpl,
  stepsByItem,
}: {
  worker: WorkerLike;
  fetchImpl: typeof globalThis.fetch;
  stepsByItem: Record<string, StepDTO[]>;
}) {
  const { openItemId, sessionSteps, open, back, confirm, turn } = useGrillTakeoverWiring(worker, stepsByItem, {
    fetch: fetchImpl,
    tokenStore: {
      read: async () => ({ token: "hb_device_token", enteredAtMs: 1_000 }),
      write: async () => {},
      clear: async () => {},
    },
  });
  return (
    <>
      <span data-testid="open">{openItemId ?? "none"}</span>
      <span data-testid="phase">{turn.phase}</span>
      <span data-testid="session-steps">{sessionSteps === null ? "null" : JSON.stringify(sessionSteps)}</span>
      <button type="button" onClick={() => open("item-1")}>open</button>
      <button type="button" onClick={back}>back</button>
      <button
        type="button"
        onClick={() =>
          confirm({
            transcript: "t",
            summary: "s",
            verdict: "resolved",
            modelProposal: "{}",
            appliedPatch: "{}",
            deleteUntickedPlan: false,
          })
        }
      >
        confirm
      </button>
      {/* Two calls in the SAME synchronous handler — what a genuine
          double click (or a double-invoked event) looks like, unlike two
          separate `fireEvent.click`s, which each flush a full React
          render in between and so can never actually race. */}
      <button
        type="button"
        onClick={() => {
          const completion = {
            transcript: "t",
            summary: "s",
            verdict: "resolved" as const,
            modelProposal: "{}",
            appliedPatch: "{}",
            deleteUntickedPlan: false,
          };
          confirm(completion);
          confirm(completion);
        }}
      >
        confirm-twice
      </button>
    </>
  );
}

async function settle(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("useGrillTakeoverWiring", () => {
  it("opening requests the item's steps and closes back to null", async () => {
    const fetchImpl = vi.fn(async () => ndjson(QUESTION_LINE));
    const worker = fakeWorker();
    render(<Harness worker={worker} fetchImpl={fetchImpl as never} stepsByItem={{}} />);

    fireEvent.click(screen.getByText("open"));
    await settle();

    expect(screen.getByTestId("open").textContent).toBe("item-1");
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "getSteps", itemId: "item-1" });

    fireEvent.click(screen.getByText("back"));
    expect(screen.getByTestId("open").textContent).toBe("none");
    expect(screen.getByTestId("phase").textContent).toBe("idle");
  });

  /** BLOCKER 2's regression test: `sessionSteps` is `null` until a FRESH
   * answer lands after `open()`, and it must not just mirror whatever
   * `stepsByItem` says on every render — the whole point is a frozen
   * snapshot, not a live read. */
  it("sessionSteps stays null until a fresh answer lands, then freezes — later live changes never reach it", async () => {
    const fetchImpl = vi.fn(async () => ndjson(QUESTION_LINE));
    const worker = fakeWorker();
    const stale = [step({ id: "stale-step" })];
    const { rerender } = render(
      <Harness worker={worker} fetchImpl={fetchImpl as never} stepsByItem={{ "item-1": stale }} />,
    );

    // A cached (stale) entry already exists for item-1 before `open()` is
    // even called — `sessionSteps` must not adopt it just because it is
    // present; only a FRESH answer (a new array reference) may seed it.
    fireEvent.click(screen.getByText("open"));
    await settle();
    expect(screen.getByTestId("session-steps").textContent).toBe("null");

    // The fresh answer lands — a genuinely new array, even with the same
    // content, is what a real `getSteps` round trip would produce.
    const fresh = [step({ id: "fresh-step" })];
    rerender(<Harness worker={worker} fetchImpl={fetchImpl as never} stepsByItem={{ "item-1": fresh }} />);
    expect(screen.getByTestId("session-steps").textContent).toBe(JSON.stringify(fresh));

    // A LATER live change (someone ticks a Step elsewhere, mid-interview)
    // must never reach the frozen snapshot.
    const laterLive = [step({ id: "fresh-step", done: true })];
    rerender(<Harness worker={worker} fetchImpl={fetchImpl as never} stepsByItem={{ "item-1": laterLive }} />);
    expect(screen.getByTestId("session-steps").textContent).toBe(JSON.stringify(fresh));
  });

  it("confirm sends the frozen snapshot, not whatever is live at submit time", async () => {
    const fetchImpl = vi.fn(async () => ndjson(PROPOSAL_LINE));
    const worker = fakeWorker();
    const frozen = [step({ id: "frozen-step" })];
    const { rerender } = render(
      <Harness worker={worker} fetchImpl={fetchImpl as never} stepsByItem={{}} />,
    );

    fireEvent.click(screen.getByText("open"));
    await settle();
    rerender(<Harness worker={worker} fetchImpl={fetchImpl as never} stepsByItem={{ "item-1": frozen }} />);
    expect(screen.getByTestId("session-steps").textContent).toBe(JSON.stringify(frozen));

    // Steps change live, right up to the moment Confirm is pressed.
    const liveAtSubmit = [step({ id: "frozen-step" }), step({ id: "new-live-step" })];
    rerender(<Harness worker={worker} fetchImpl={fetchImpl as never} stepsByItem={{ "item-1": liveAtSubmit }} />);

    fireEvent.click(screen.getByText("confirm"));

    const completeGrillCall = worker.postMessage.mock.calls.find(([m]) => m.type === "completeGrill");
    expect(completeGrillCall?.[0]?.sessionSteps).toEqual(frozen);
  });

  it("confirm is a no-op until the snapshot lands, and closes the takeover on submit", async () => {
    const fetchImpl = vi.fn(async () => ndjson(PROPOSAL_LINE));
    const worker = fakeWorker();
    render(<Harness worker={worker} fetchImpl={fetchImpl as never} stepsByItem={{}} />);

    fireEvent.click(screen.getByText("open"));
    await settle();

    // No snapshot yet (`stepsByItem` never answered for item-1) — confirm
    // must send nothing.
    fireEvent.click(screen.getByText("confirm"));
    expect(worker.postMessage.mock.calls.some(([m]) => m.type === "completeGrill")).toBe(false);
  });

  it("a double-click on confirm mints only one completeGrill message", async () => {
    const fetchImpl = vi.fn(async () => ndjson(PROPOSAL_LINE));
    const worker = fakeWorker();
    const { rerender } = render(
      <Harness worker={worker} fetchImpl={fetchImpl as never} stepsByItem={{}} />,
    );

    fireEvent.click(screen.getByText("open"));
    await settle();
    rerender(<Harness worker={worker} fetchImpl={fetchImpl as never} stepsByItem={{ "item-1": [] }} />);

    fireEvent.click(screen.getByText("confirm"));
    fireEvent.click(screen.getByText("confirm"));

    expect(worker.postMessage.mock.calls.filter(([m]) => m.type === "completeGrill")).toHaveLength(1);
    // Optimistic close — the takeover no longer holds this session open.
    expect(screen.getByTestId("open").textContent).toBe("none");
  });

  /** The real regression: two calls inside the SAME synchronous handler,
   * before React has had any chance to re-render and unmount the button
   * or update `openItemId` — the case a per-click `disabled` prop can
   * never cover on its own, and the reason `confirm` needs its own
   * synchronous lock (`useGrillWiring.ts`'s `inFlight` is the same shape,
   * for the identical reason). */
  it("two confirm() calls in the same tick still mint only one Grill", async () => {
    const fetchImpl = vi.fn(async () => ndjson(PROPOSAL_LINE));
    const worker = fakeWorker();
    const { rerender } = render(
      <Harness worker={worker} fetchImpl={fetchImpl as never} stepsByItem={{}} />,
    );

    fireEvent.click(screen.getByText("open"));
    await settle();
    rerender(<Harness worker={worker} fetchImpl={fetchImpl as never} stepsByItem={{ "item-1": [] }} />);

    fireEvent.click(screen.getByText("confirm-twice"));

    expect(worker.postMessage.mock.calls.filter(([m]) => m.type === "completeGrill")).toHaveLength(1);
  });
});
