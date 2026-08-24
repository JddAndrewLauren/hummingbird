// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import type { CoreStatus } from "../store/store";
import type { WorkerLike } from "../store/worker-client";
import { render } from "../test/component";
import { useFrontierWiring } from "./useFrontierWiring";

// Mounted rather than called, for `useLedgerWiring.test.tsx`'s own reason: a
// hook that is exported, unit-tested and never wired compiles clean and does
// nothing.

function fakeWorker(): WorkerLike & { postMessage: ReturnType<typeof vi.fn> } {
  return { onmessage: null, postMessage: vi.fn() };
}

function Probe({
  worker,
  status,
  syncOutcomeSeq,
}: {
  worker: WorkerLike;
  status: CoreStatus;
  syncOutcomeSeq: number;
}) {
  useFrontierWiring(worker, status, syncOutcomeSeq);
  return null;
}

function types(worker: ReturnType<typeof fakeWorker>): string[] {
  return worker.postMessage.mock.calls.map(([message]) => (message as { type: string }).type);
}

describe("useFrontierWiring", () => {
  it("asks nothing while the core is still loading", () => {
    const worker = fakeWorker();
    render(<Probe worker={worker} status="loading" syncOutcomeSeq={0} />);
    expect(worker.postMessage).not.toHaveBeenCalled();
  });

  it("requests the frontier, blocked, projects, grilling and externally-blocked reads once the core is ready", () => {
    const worker = fakeWorker();
    render(<Probe worker={worker} status="ready" syncOutcomeSeq={0} />);
    expect(types(worker)).toEqual([
      "getFrontier",
      "getBlocked",
      "getProjects",
      "getGrillingItems",
      "getExternallyBlocked",
    ]);
  });

  it("re-requests on every completed cycle", () => {
    const worker = fakeWorker();
    const view = render(<Probe worker={worker} status="ready" syncOutcomeSeq={0} />);
    view.rerender(<Probe worker={worker} status="ready" syncOutcomeSeq={1} />);
    expect(types(worker)).toEqual([
      "getFrontier",
      "getBlocked",
      "getProjects",
      "getGrillingItems",
      "getExternallyBlocked",
      "getFrontier",
      "getBlocked",
      "getProjects",
      "getGrillingItems",
      "getExternallyBlocked",
    ]);
  });

  it("does not re-request on a render that changed nothing", () => {
    const worker = fakeWorker();
    const view = render(<Probe worker={worker} status="ready" syncOutcomeSeq={2} />);
    view.rerender(<Probe worker={worker} status="ready" syncOutcomeSeq={2} />);
    expect(types(worker)).toEqual([
      "getFrontier",
      "getBlocked",
      "getProjects",
      "getGrillingItems",
      "getExternallyBlocked",
    ]);
  });
});
