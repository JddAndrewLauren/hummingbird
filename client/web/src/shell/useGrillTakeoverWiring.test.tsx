// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "../test/component";
import type { WorkerLike } from "../store/worker-client";
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

function Harness({ worker, fetchImpl }: { worker: WorkerLike; fetchImpl: typeof globalThis.fetch }) {
  const { openItemId, open, back, turn } = useGrillTakeoverWiring(worker, 0, {
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
      <button type="button" onClick={() => open("item-1")}>open</button>
      <button type="button" onClick={back}>back</button>
    </>
  );
}

describe("useGrillTakeoverWiring", () => {
  it("opening requests the item's steps and closes back to null", async () => {
    const fetchImpl = vi.fn(async () => ndjson(QUESTION_LINE));
    const worker = fakeWorker();
    render(<Harness worker={worker} fetchImpl={fetchImpl as never} />);

    fireEvent.click(screen.getByText("open"));
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(screen.getByTestId("open").textContent).toBe("item-1");
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "getSteps", itemId: "item-1" });

    fireEvent.click(screen.getByText("back"));
    expect(screen.getByTestId("open").textContent).toBe("none");
    expect(screen.getByTestId("phase").textContent).toBe("idle");
  });
});
