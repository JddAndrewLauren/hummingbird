// @vitest-environment jsdom
//
// The behaviours that only exist once the hook is mounted: asking opens
// with an empty `turns`, answering appends the round and asks again,
// "Keep grilling" re-asks with the same turns, and a second tap while
// asking starts no second request — `useMicrotaskWiring.test.tsx`'s own
// shape, for the Grill turn lane instead of the microtask one.

import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "../test/component";
import type { TaskTokenStoreLike } from "../task/token-store";
import { useGrillWiring } from "./useGrillWiring";

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
  fetchImpl,
  store,
  itemId = "item-1",
}: {
  fetchImpl: typeof globalThis.fetch;
  store: TaskTokenStoreLike;
  itemId?: string;
}) {
  const { turn, turns, onAsk, onAnswer, onKeepGrilling, onDiscard } = useGrillWiring(itemId, {
    fetch: fetchImpl,
    tokenStore: store,
  });
  return (
    <>
      <span data-testid="phase">{turn.phase}</span>
      <span data-testid="turn-count">{turns.length}</span>
      {turn.phase === "question" ? <span data-testid="prompt">{turn.question.prompt}</span> : null}
      {turn.phase === "proposal" ? <span data-testid="summary">{turn.proposal.summary}</span> : null}
      <button type="button" onClick={() => onAsk(itemId, itemId)}>ask</button>
      <button type="button" onClick={() => onAnswer(itemId, itemId, "SEA")}>answer</button>
      <button type="button" onClick={() => onKeepGrilling(itemId, itemId)}>keep grilling</button>
      <button type="button" onClick={() => onDiscard(itemId)}>discard</button>
    </>
  );
}

function phase(): string {
  return screen.getByTestId("phase").textContent ?? "";
}

async function settle(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
  await new Promise((resolve) => setTimeout(resolve, 0));
}

const QUESTION_LINE =
  '{"ok":true,"skill":"grill-me","result":{"kind":"question","question":{"prompt":"Which airport?","recommendedAnswer":"SEA","choices":["SEA","PDX"]}},"backend":"cloud","model":"opus"}';
const PROPOSAL_LINE =
  '{"ok":true,"skill":"grill-me","result":{"kind":"proposal","proposal":{"summary":"Settled on SEA","verdict":"resolved","patch":{}}},"backend":"cloud","model":"opus"}';

describe("useGrillWiring", () => {
  it("onAsk opens the interview with an empty turns array against /api/skills/run", async () => {
    const fetchImpl = vi.fn(async (_input: unknown, _init?: RequestInit) => ndjson(QUESTION_LINE));
    render(<Harness fetchImpl={fetchImpl as never} store={tokenStore()} />);

    fireEvent.click(screen.getByText("ask"));
    await settle();

    expect(fetchImpl).toHaveBeenCalledTimes(1);
    expect(fetchImpl.mock.calls[0]?.[0]).toBe("/api/skills/run");
    const body = JSON.parse(String(fetchImpl.mock.calls[0]?.[1]?.body));
    expect(body).toEqual({ skill: "grill-me", args: { ref: "item-1", turns: [] } });
    expect(phase()).toBe("question");
    expect(screen.getByTestId("prompt").textContent).toBe("Which airport?");
  });

  it("onAnswer appends the round and asks again with the accumulated turns", async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValueOnce(ndjson(QUESTION_LINE))
      .mockResolvedValueOnce(ndjson(PROPOSAL_LINE));
    render(<Harness fetchImpl={fetchImpl as never} store={tokenStore()} />);

    fireEvent.click(screen.getByText("ask"));
    await settle();
    fireEvent.click(screen.getByText("answer"));
    await settle();

    expect(fetchImpl).toHaveBeenCalledTimes(2);
    const secondBody = JSON.parse(String((fetchImpl.mock.calls[1] as [unknown, RequestInit])[1].body));
    expect(secondBody.args.turns).toEqual([
      { question: { prompt: "Which airport?", recommendedAnswer: "SEA", choices: ["SEA", "PDX"] }, answer: "SEA" },
    ]);
    expect(phase()).toBe("proposal");
    expect(screen.getByTestId("summary").textContent).toBe("Settled on SEA");
    expect(screen.getByTestId("turn-count").textContent).toBe("1");
  });

  it("onKeepGrilling re-asks with the SAME turns, appending nothing new", async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValueOnce(ndjson(PROPOSAL_LINE))
      .mockResolvedValueOnce(ndjson(QUESTION_LINE));
    render(<Harness fetchImpl={fetchImpl as never} store={tokenStore()} />);

    fireEvent.click(screen.getByText("ask"));
    await settle();
    expect(phase()).toBe("proposal");

    fireEvent.click(screen.getByText("keep grilling"));
    await settle();

    expect(fetchImpl).toHaveBeenCalledTimes(2);
    const secondBody = JSON.parse(String((fetchImpl.mock.calls[1] as [unknown, RequestInit])[1].body));
    expect(secondBody.args.turns).toEqual([]);
    expect(phase()).toBe("question");
  });

  it("a second ask while asking starts no second request", async () => {
    let release: () => void = () => {};
    const fetchImpl = vi.fn(async () => {
      const body = new ReadableStream<Uint8Array>({
        start(controller) {
          controller.enqueue(new TextEncoder().encode('{"type":"progress","message":"reading"}\n'));
          release = () => {
            controller.enqueue(new TextEncoder().encode(`${QUESTION_LINE}\n`));
            controller.close();
          };
        },
      });
      return new Response(body, { status: 200 });
    });
    render(<Harness fetchImpl={fetchImpl as never} store={tokenStore()} />);

    fireEvent.click(screen.getByText("ask"));
    await settle();
    expect(phase()).toBe("asking");

    fireEvent.click(screen.getByText("ask"));
    await settle();
    expect(fetchImpl).toHaveBeenCalledTimes(1);

    release();
    await settle();
  });

  it("onDiscard clears the turn state for that item", async () => {
    const fetchImpl = vi.fn(async () => ndjson(QUESTION_LINE));
    render(<Harness fetchImpl={fetchImpl as never} store={tokenStore()} />);

    fireEvent.click(screen.getByText("ask"));
    await settle();
    expect(phase()).toBe("question");

    fireEvent.click(screen.getByText("discard"));
    await settle();
    expect(phase()).toBe("idle");
    expect(screen.getByTestId("turn-count").textContent).toBe("0");
  });

  it("with no token stored, asking declines without a request", async () => {
    const fetchImpl = vi.fn();
    render(<Harness fetchImpl={fetchImpl as never} store={tokenStore(null)} />);

    fireEvent.click(screen.getByText("ask"));
    await settle();

    expect(fetchImpl).not.toHaveBeenCalled();
    expect(phase()).toBe("declined");
  });
});
