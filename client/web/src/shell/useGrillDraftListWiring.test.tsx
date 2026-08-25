// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import type { CoreStatus } from "../store/store";
import type { WorkerLike } from "../store/worker-client";
import { render } from "../test/component";
import { useGrillDraftListWiring } from "./useGrillDraftListWiring";

// Mounted rather than called, for `useLedgerWiring.test.tsx`'s own reason: a
// hook that is exported, unit-tested and never wired compiles clean and does
// nothing.

function fakeWorker(): WorkerLike & { postMessage: ReturnType<typeof vi.fn> } {
  return { onmessage: null, postMessage: vi.fn() };
}

function Probe({ worker, status }: { worker: WorkerLike; status: CoreStatus }) {
  useGrillDraftListWiring(worker, status);
  return null;
}

function types(worker: ReturnType<typeof fakeWorker>): string[] {
  return worker.postMessage.mock.calls.map(([message]) => (message as { type: string }).type);
}

describe("useGrillDraftListWiring", () => {
  it("asks nothing while the core is still loading", () => {
    const worker = fakeWorker();
    render(<Probe worker={worker} status="loading" />);
    expect(worker.postMessage).not.toHaveBeenCalled();
  });

  it("requests the draft item ids once the core is ready", () => {
    const worker = fakeWorker();
    render(<Probe worker={worker} status="ready" />);
    expect(types(worker)).toEqual(["getGrillDraftItemIds"]);
  });

  it("does not re-request on a later render — a draft never rides a sync cycle", () => {
    const worker = fakeWorker();
    const view = render(<Probe worker={worker} status="ready" />);
    view.rerender(<Probe worker={worker} status="ready" />);
    expect(types(worker)).toEqual(["getGrillDraftItemIds"]);
  });
});
