import { describe, expect, it, vi } from "vitest";
import type { WorkerLike } from "../store/worker-client";
import { submitCaptureRequest } from "./useCaptureWiring";

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

describe("submitCaptureRequest", () => {
  it("posts a getTriageInbox request immediately after the capture — this must fail without the fix", () => {
    const worker = fakeWorker();

    submitCaptureRequest(worker, "buy milk", 1_000, "seed-1");

    const types = worker.postMessage.mock.calls.map(([message]) => message.type);
    expect(types).toEqual(["capture", "getTriageInbox"]);
  });

  it("the capture message itself carries the raw title and stage unmodified", () => {
    const worker = fakeWorker();

    submitCaptureRequest(worker, "  Buy   OAT milk  ", 5_000, "seed-2");

    expect(worker.postMessage).toHaveBeenNthCalledWith(1, {
      type: "capture",
      seed: "seed-2",
      title: "  Buy   OAT milk  ",
      stage: "triage",
      nowMs: 5_000,
    });
  });

  it("mints a distinct seed per call when none is supplied", () => {
    const worker = fakeWorker();

    submitCaptureRequest(worker, "first", 1_000);
    submitCaptureRequest(worker, "second", 2_000);

    const seeds = worker.postMessage.mock.calls
      .map(([message]) => message)
      .filter((message) => message.type === "capture")
      .map((message) => (message as { seed: string }).seed);
    expect(seeds[0]).not.toEqual(seeds[1]);
  });
});
