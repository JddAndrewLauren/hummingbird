import { afterEach, describe, expect, it, vi } from "vitest";
import type { WorkerLike } from "../store/worker-client";
import { mintSeed, submitCaptureRequest } from "./useCaptureWiring";

// Round-2 review of PR #206: acceptance criterion 1 ("A capture is visible
// in the list before any network call") did not reach runtime — nothing
// re-read the triage inbox after a capture, so the optimistic overlay was
// invisible until the next 60s ADR-0007 timer tick, which arrives AFTER a
// network attempt. `submitCaptureRequest` is the fix: it re-requests the
// inbox immediately behind the capture post, ordered by `task-worker.ts`'s
// serial queue.
function fakeWorker(): WorkerLike & { postMessage: ReturnType<typeof vi.fn> } {
  return {
    onmessage: null,
    postMessage: vi.fn(),
  };
}

/** Every capture field at its resting state — the wire spells all of them out
 * as `null` even when the caller passed `{}`. */
const EMPTY_CAPTURE_FIELDS = {
  size: null,
  energy: null,
  context: null,
  description: null,
  projectId: null,
  priority: null,
  deadline: null,
  scheduledDate: null,
};

describe("submitCaptureRequest", () => {
  it("posts a getTriageInbox request immediately after the capture — this must fail without the fix", () => {
    const worker = fakeWorker();

    submitCaptureRequest(worker, "buy milk", "triage", 1_000, {}, "seed-1");

    const types = worker.postMessage.mock.calls.map(([message]) => message.type);
    expect(types).toEqual(["capture", "getTriageInbox"]);
  });

  it("the capture message itself carries the raw title and stage unmodified, with every field absent by default", () => {
    const worker = fakeWorker();

    submitCaptureRequest(worker, "  Buy   OAT milk  ", "triage", 5_000, {}, "seed-2");

    expect(worker.postMessage).toHaveBeenNthCalledWith(1, {
      type: "capture",
      seed: "seed-2",
      title: "  Buy   OAT milk  ",
      stage: "triage",
      fields: EMPTY_CAPTURE_FIELDS,
      nowMs: 5_000,
    });
  });

  // #208: a caller-supplied `fields` reaches the capture message verbatim.
  it("carries every set field onto the same capture message", () => {
    const worker = fakeWorker();

    submitCaptureRequest(
      worker,
      "buy milk",
      "triage",
      5_000,
      {
        size: "deep",
        energy: "high",
        context: "@errands",
        description: "the oat kind",
        projectId: "proj-1",
        priority: 3,
        deadline: "2026-09-01",
        scheduledDate: "2026-08-30",
      },
      "seed-3",
    );

    expect(worker.postMessage).toHaveBeenNthCalledWith(1, {
      type: "capture",
      seed: "seed-3",
      title: "buy milk",
      stage: "triage",
      fields: {
        size: "deep",
        energy: "high",
        context: "@errands",
        description: "the oat kind",
        projectId: "proj-1",
        priority: 3,
        deadline: "2026-09-01",
        scheduledDate: "2026-08-30",
      },
      nowMs: 5_000,
    });
  });

  it("a minted capture carries stage `ready` and re-reads the frontier, not just the inbox", () => {
    // The skip-triage button (`screens/capture-destination.ts`). The item is
    // born past triage, so the inbox re-read alone would leave the thing just
    // typed invisible until the next 60s cycle — the same gap the inbox
    // re-read above exists to close, one query over.
    const worker = fakeWorker();

    submitCaptureRequest(worker, "Order the worktop", "ready", 7_000, {}, "seed-3");

    expect(worker.postMessage.mock.calls[0][0]).toEqual({
      type: "capture",
      seed: "seed-3",
      title: "Order the worktop",
      stage: "ready",
      fields: EMPTY_CAPTURE_FIELDS,
      nowMs: 7_000,
    });
    const types = worker.postMessage.mock.calls.map(([message]) => message.type);
    expect(types).toEqual(["capture", "getTriageInbox", "getFrontier"]);
  });

  it("mints a distinct seed per call when none is supplied", () => {
    const worker = fakeWorker();

    submitCaptureRequest(worker, "first", "triage", 1_000);
    submitCaptureRequest(worker, "second", "triage", 2_000);

    const seeds = worker.postMessage.mock.calls
      .map(([message]) => message)
      .filter((message) => message.type === "capture")
      .map((message) => (message as { seed: string }).seed);
    expect(seeds[0]).not.toEqual(seeds[1]);
  });
});

// #223: pins the non-deterministic half of the sync module's seed-minting
// rule (client/core/src/sync/mod.rs) — a capture mints a *new* item, its
// seed's hash becomes the item's id on the authority's client-id-keyed
// create path, so two captures of identical text must never collide into
// one item.
describe("mintSeed", () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  // Through the real submit path, with the title AND the clock identical —
  // `mintSeed()` itself takes neither, so only a whole-message assertion can
  // actually claim "identical text".
  it("two captures of identical text at the same nowMs mint different seeds", () => {
    const worker = fakeWorker();

    submitCaptureRequest(worker, "buy milk", "triage", 1_000);
    submitCaptureRequest(worker, "buy milk", "triage", 1_000);

    const captures = worker.postMessage.mock.calls
      .map(([message]) => message)
      .filter((message) => message.type === "capture") as Array<{
      title: string;
      seed: string;
    }>;
    expect(captures.map((message) => message.title)).toEqual(["buy milk", "buy milk"]);
    expect(captures[0].seed).not.toEqual(captures[1].seed);
  });

  // The arm that could actually collide: without `crypto.randomUUID`, the
  // fallback composes `Date.now()` (frozen here via a fake timer, so two
  // calls land in the same millisecond — exactly the case the fallback's
  // own comment names) with `Math.random()`, which is what must carry the
  // uniqueness burden alone.
  it("the crypto.randomUUID-less fallback still mints distinct seeds for two captures in the same millisecond", () => {
    vi.stubGlobal("crypto", {});
    vi.useFakeTimers();
    vi.setSystemTime(1_000);

    const first = mintSeed();
    const second = mintSeed();

    expect(first).not.toEqual(second);
  });
});
