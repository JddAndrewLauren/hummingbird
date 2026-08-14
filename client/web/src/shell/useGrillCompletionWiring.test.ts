import { describe, expect, it, vi } from "vitest";
import type { WorkerLike } from "../store/worker-client";
import { mintGrillCompletionSeed, useGrillCompletionWiring } from "./useGrillCompletionWiring";

describe("mintGrillCompletionSeed", () => {
  it("retrying the same completion (same item, nowMs) mints the same seed", () => {
    const first = mintGrillCompletionSeed("item-1", 5_000);
    const second = mintGrillCompletionSeed("item-1", 5_000);

    expect(first).toEqual(second);
  });

  it("a different item mints a different seed", () => {
    const a = mintGrillCompletionSeed("item-1", 5_000);
    const b = mintGrillCompletionSeed("item-2", 5_000);

    expect(a).not.toEqual(b);
  });
});

describe("useGrillCompletionWiring", () => {
  it("posts a completeGrill message carrying every field, and returns the seed it sent", () => {
    const worker: WorkerLike = { onmessage: null, postMessage: vi.fn() };
    const { completeGrill } = useGrillCompletionWiring(worker);

    const seed = completeGrill(
      "item-1",
      [],
      {
        transcript: "Q: destination?\nA: Tokyo",
        summary: "Settled on Tokyo",
        verdict: "resolved",
        modelProposal: '{"title":"book flights"}',
        appliedPatch: '{"title":"book flights"}',
        deleteUntickedPlan: false,
      },
    );

    expect(worker.postMessage).toHaveBeenCalledTimes(1);
    const [message] = (worker.postMessage as ReturnType<typeof vi.fn>).mock.calls[0];
    expect(message).toMatchObject({
      type: "completeGrill",
      itemId: "item-1",
      sessionSteps: [],
      transcript: "Q: destination?\nA: Tokyo",
      summary: "Settled on Tokyo",
      verdict: "resolved",
      modelProposal: '{"title":"book flights"}',
      appliedPatch: '{"title":"book flights"}',
      deleteUntickedPlan: false,
    });
    expect(typeof message.seed).toBe("string");
    expect(typeof message.nowMs).toBe("number");
    // The caller matches the answering broadcast on exactly this — a
    // returned seed that differed from the sent one would leave the takeover
    // waiting on an answer that never comes.
    expect(seed).toBe(message.seed);
  });
});
