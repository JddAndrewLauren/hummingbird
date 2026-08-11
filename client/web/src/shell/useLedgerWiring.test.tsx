// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import type { CoreStatus, TaskState } from "../store/store";
import type { WorkerLike } from "../store/worker-client";
import { render } from "../test/component";
import { useLedgerWiring } from "./useLedgerWiring";

// Mounted rather than called, for `usePaneReadsWiring.test.tsx`'s own reason:
// a hook that is exported, unit-tested and never wired compiles clean and
// does nothing.

function fakeWorker(): WorkerLike & { postMessage: ReturnType<typeof vi.fn> } {
  return { onmessage: null, postMessage: vi.fn() };
}

function Probe({
  worker,
  status,
  syncOutcomeSeq,
  lastCapture = null,
  lastAct = null,
  lastTriage = null,
}: {
  worker: WorkerLike;
  status: CoreStatus;
  syncOutcomeSeq: number;
  lastCapture?: TaskState["lastCapture"];
  lastAct?: TaskState["lastAct"];
  lastTriage?: TaskState["lastTriage"];
}) {
  useLedgerWiring(worker, status, syncOutcomeSeq, lastCapture, lastAct, lastTriage);
  return null;
}

function types(worker: ReturnType<typeof fakeWorker>): string[] {
  return worker.postMessage.mock.calls.map(([message]) => (message as { type: string }).type);
}

describe("useLedgerWiring", () => {
  it("asks nothing while the core is still loading", () => {
    const worker = fakeWorker();
    render(<Probe worker={worker} status="loading" syncOutcomeSeq={0} />);
    expect(worker.postMessage).not.toHaveBeenCalled();
  });

  it("requests the ledger (with a nowMs) and the done list once the core is ready", () => {
    const worker = fakeWorker();
    render(<Probe worker={worker} status="ready" syncOutcomeSeq={0} />);
    expect(types(worker)).toEqual(["getLedger", "getDone"]);
    const [ledgerMessage] = worker.postMessage.mock.calls[0] as [{ type: string; nowMs: number }];
    expect(typeof ledgerMessage.nowMs).toBe("number");
  });

  it("re-requests on every completed cycle", () => {
    const worker = fakeWorker();
    const view = render(<Probe worker={worker} status="ready" syncOutcomeSeq={0} />);
    view.rerender(<Probe worker={worker} status="ready" syncOutcomeSeq={1} />);
    expect(types(worker)).toEqual(["getLedger", "getDone", "getLedger", "getDone"]);
  });

  it("re-requests when a mutation result lands — an offline complete shows without a cycle", () => {
    const worker = fakeWorker();
    const view = render(<Probe worker={worker} status="ready" syncOutcomeSeq={0} />);
    view.rerender(
      <Probe
        worker={worker}
        status="ready"
        syncOutcomeSeq={0}
        lastAct={{ seed: "s-1", itemId: "a-1", action: "complete", kind: "ok", error: null }}
      />,
    );
    expect(types(worker)).toEqual(["getLedger", "getDone", "getLedger", "getDone"]);
  });

  it("does not re-request on a render that changed nothing", () => {
    const worker = fakeWorker();
    const view = render(<Probe worker={worker} status="ready" syncOutcomeSeq={2} />);
    view.rerender(<Probe worker={worker} status="ready" syncOutcomeSeq={2} />);
    expect(types(worker)).toEqual(["getLedger", "getDone"]);
  });
});
