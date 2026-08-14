// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "../test/component";
import type { WorkerLike } from "../store/worker-client";
import type { StepDTO } from "../store/protocol";
import type { TaskGrillCompletionResult } from "../store/store";
import { grillCompletionFailureFor } from "../screens/write-failure";
import { useGrillTakeoverWiring } from "./useGrillTakeoverWiring";

/** The `completeGrillResult` broadcast for whichever seed the harness's
 * confirm just minted — read back off the worker message rather than
 * recomputed, so a test can never assert against a seed the hook did not
 * actually send. */
function resultFor(
  worker: { postMessage: ReturnType<typeof vi.fn> },
  kind: TaskGrillCompletionResult["kind"],
  error: string | null = null,
): TaskGrillCompletionResult {
  const sent = worker.postMessage.mock.calls.find(([m]) => m.type === "completeGrill")?.[0];
  return { seed: sent.seed, itemId: sent.itemId, kind, grillId: null, error };
}

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
  lastGrillCompletion = null,
}: {
  worker: WorkerLike;
  fetchImpl: typeof globalThis.fetch;
  stepsByItem: Record<string, StepDTO[]>;
  lastGrillCompletion?: TaskGrillCompletionResult | null;
}) {
  const { openItemId, sessionSteps, open, back, confirm, keepGrilling, confirmSeed, turn } =
    useGrillTakeoverWiring(worker, stepsByItem, lastGrillCompletion, {
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
      <span data-testid="confirm-seed">{confirmSeed ?? "none"}</span>
      <button type="button" onClick={() => open("item-1")}>open</button>
      <button type="button" onClick={back}>back</button>
      <button type="button" onClick={keepGrilling}>keep-grilling</button>
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

  it("confirm is a no-op until the snapshot lands", async () => {
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
    // The takeover is still up: nothing has answered yet, and the close
    // waits for an `"ok"`. The second click was stopped by the lock, not by
    // the button going away.
    expect(screen.getByTestId("open").textContent).toBe("item-1");
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

  /** ROUND 2's blocker: `Core::complete_grill` refuses to enqueue at all on
   * a drift (`needs_re_review`, #354's guard), and that refusal has nowhere
   * to be seen if the takeover has already torn itself down. The takeover
   * must still be standing — transcript and all — when the refusal lands. */
  it("a refused confirm leaves the takeover open, with the refusal named against this confirm's seed", async () => {
    const fetchImpl = vi.fn(async () => ndjson(PROPOSAL_LINE));
    const worker = fakeWorker();
    const { rerender } = render(
      <Harness worker={worker} fetchImpl={fetchImpl as never} stepsByItem={{}} />,
    );

    fireEvent.click(screen.getByText("open"));
    await settle();
    rerender(<Harness worker={worker} fetchImpl={fetchImpl as never} stepsByItem={{ "item-1": [] }} />);
    fireEvent.click(screen.getByText("confirm"));

    const seed = screen.getByTestId("confirm-seed").textContent;
    expect(seed).not.toBe("none");

    const refusal = resultFor(worker, "needs_re_review", "unticked steps changed since this review was last shown");
    rerender(
      <Harness
        worker={worker}
        fetchImpl={fetchImpl as never}
        stepsByItem={{ "item-1": [] }}
        lastGrillCompletion={refusal}
      />,
    );

    // Still open, still on the proposal — the review the guard exists to
    // force is there to be re-read.
    expect(screen.getByTestId("open").textContent).toBe("item-1");
    expect(screen.getByTestId("phase").textContent).toBe("proposal");
    // And the seed survives, which is what keeps the message on screen.
    expect(screen.getByTestId("confirm-seed").textContent).toBe(seed);
    expect(grillCompletionFailureFor(refusal, seed)).toBe(
      "unticked steps changed since this review was last shown",
    );
  });

  it("a refused confirm releases the lock, so the same session can confirm again", async () => {
    const fetchImpl = vi.fn(async () => ndjson(PROPOSAL_LINE));
    const worker = fakeWorker();
    const { rerender } = render(
      <Harness worker={worker} fetchImpl={fetchImpl as never} stepsByItem={{}} />,
    );

    fireEvent.click(screen.getByText("open"));
    await settle();
    rerender(<Harness worker={worker} fetchImpl={fetchImpl as never} stepsByItem={{ "item-1": [] }} />);
    fireEvent.click(screen.getByText("confirm"));

    rerender(
      <Harness
        worker={worker}
        fetchImpl={fetchImpl as never}
        stepsByItem={{ "item-1": [] }}
        lastGrillCompletion={resultFor(worker, "failed", "network")}
      />,
    );

    fireEvent.click(screen.getByText("confirm"));
    expect(worker.postMessage.mock.calls.filter(([m]) => m.type === "completeGrill")).toHaveLength(2);
  });

  it("an ok confirm closes the takeover and discards the turn state", async () => {
    const fetchImpl = vi.fn(async () => ndjson(PROPOSAL_LINE));
    const worker = fakeWorker();
    const { rerender } = render(
      <Harness worker={worker} fetchImpl={fetchImpl as never} stepsByItem={{}} />,
    );

    fireEvent.click(screen.getByText("open"));
    await settle();
    rerender(<Harness worker={worker} fetchImpl={fetchImpl as never} stepsByItem={{ "item-1": [] }} />);
    fireEvent.click(screen.getByText("confirm"));
    expect(screen.getByTestId("open").textContent).toBe("item-1");

    rerender(
      <Harness
        worker={worker}
        fetchImpl={fetchImpl as never}
        stepsByItem={{ "item-1": [] }}
        lastGrillCompletion={resultFor(worker, "ok")}
      />,
    );

    expect(screen.getByTestId("open").textContent).toBe("none");
    expect(screen.getByTestId("phase").textContent).toBe("idle");
    expect(screen.getByTestId("session-steps").textContent).toBe("null");
  });

  /** A result for somebody ELSE's confirm must not close this session's
   * takeover — `lastGrillCompletion` holds one slot for the whole app. */
  it("a completion result for another seed is ignored", async () => {
    const fetchImpl = vi.fn(async () => ndjson(PROPOSAL_LINE));
    const worker = fakeWorker();
    const { rerender } = render(
      <Harness worker={worker} fetchImpl={fetchImpl as never} stepsByItem={{}} />,
    );

    fireEvent.click(screen.getByText("open"));
    await settle();
    rerender(<Harness worker={worker} fetchImpl={fetchImpl as never} stepsByItem={{ "item-1": [] }} />);
    fireEvent.click(screen.getByText("confirm"));

    rerender(
      <Harness
        worker={worker}
        fetchImpl={fetchImpl as never}
        stepsByItem={{ "item-1": [] }}
        lastGrillCompletion={{ seed: "someone-else", itemId: "item-1", kind: "ok", grillId: "g-9", error: null }}
      />,
    );

    expect(screen.getByTestId("open").textContent).toBe("item-1");
  });

  /** The stale-error facet the round-2 verdict named: a failure must not
   * follow the item into the NEXT session, whose proposal it does not
   * describe. */
  it("re-opening the same item drops the previous session's confirm seed", async () => {
    const fetchImpl = vi.fn(async () => ndjson(PROPOSAL_LINE));
    const worker = fakeWorker();
    const { rerender } = render(
      <Harness worker={worker} fetchImpl={fetchImpl as never} stepsByItem={{}} />,
    );

    fireEvent.click(screen.getByText("open"));
    await settle();
    rerender(<Harness worker={worker} fetchImpl={fetchImpl as never} stepsByItem={{ "item-1": [] }} />);
    fireEvent.click(screen.getByText("confirm"));

    const stale = resultFor(worker, "needs_re_review", "unticked steps changed");
    rerender(
      <Harness
        worker={worker}
        fetchImpl={fetchImpl as never}
        stepsByItem={{ "item-1": [] }}
        lastGrillCompletion={stale}
      />,
    );
    expect(screen.getByTestId("confirm-seed").textContent).toBe(stale.seed);

    // Back out and grill the same item again: the failure is last session's.
    fireEvent.click(screen.getByText("back"));
    fireEvent.click(screen.getByText("open"));
    await settle();

    expect(screen.getByTestId("confirm-seed").textContent).toBe("none");
    expect(grillCompletionFailureFor(stale, null)).toBeNull();
  });

  it("keep grilling clears a failed confirm's error along with its proposal", async () => {
    const fetchImpl = vi.fn(async () => ndjson(PROPOSAL_LINE));
    const worker = fakeWorker();
    const { rerender } = render(
      <Harness worker={worker} fetchImpl={fetchImpl as never} stepsByItem={{}} />,
    );

    fireEvent.click(screen.getByText("open"));
    await settle();
    rerender(<Harness worker={worker} fetchImpl={fetchImpl as never} stepsByItem={{ "item-1": [] }} />);
    fireEvent.click(screen.getByText("confirm"));
    rerender(
      <Harness
        worker={worker}
        fetchImpl={fetchImpl as never}
        stepsByItem={{ "item-1": [] }}
        lastGrillCompletion={resultFor(worker, "needs_re_review", "unticked steps changed")}
      />,
    );

    fireEvent.click(screen.getByText("keep-grilling"));
    expect(screen.getByTestId("confirm-seed").textContent).toBe("none");
  });
});
