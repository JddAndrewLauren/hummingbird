// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import { requiredSources } from "../screens/questions/registry";
import type { CoreStatus } from "../store/store";
import type { WorkerLike } from "../store/worker-client";
import { render } from "../test/component";
import { usePaneReadsWiring } from "./usePaneReadsWiring";

// Mounted rather than called: a hook that is exported, unit-tested and never
// wired compiles clean and does nothing, which is the failure this app's
// component tests exist to catch.

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
  usePaneReadsWiring(worker, status, syncOutcomeSeq);
  return null;
}

function sources(worker: ReturnType<typeof fakeWorker>): string[] {
  return worker.postMessage.mock.calls
    .map(([message]) => message as { type: string; source?: string })
    .filter((message) => message.type === "getPaneRead")
    .map((message) => message.source ?? "");
}

describe("usePaneReadsWiring", () => {
  it("asks nothing while the core is still loading", () => {
    const worker = fakeWorker();
    render(<Probe worker={worker} status="loading" syncOutcomeSeq={0} />);
    expect(worker.postMessage).not.toHaveBeenCalled();
  });

  it("requests every source the registry needs once the core is ready", () => {
    const worker = fakeWorker();
    render(<Probe worker={worker} status="ready" syncOutcomeSeq={0} />);
    expect(sources(worker)).toEqual(requiredSources());
  });

  it("re-requests them on every completed cycle", () => {
    // A cycle is exactly when a new snapshot row or a new alert can have
    // arrived — and the answer's ages are measured against the request's own
    // clock, so a stale request is a stale age.
    const worker = fakeWorker();
    const view = render(<Probe worker={worker} status="ready" syncOutcomeSeq={0} />);
    view.rerender(<Probe worker={worker} status="ready" syncOutcomeSeq={1} />);
    expect(sources(worker)).toEqual([...requiredSources(), ...requiredSources()]);
  });

  it("does not re-request on a render that changed nothing", () => {
    const worker = fakeWorker();
    const view = render(<Probe worker={worker} status="ready" syncOutcomeSeq={3} />);
    view.rerender(<Probe worker={worker} status="ready" syncOutcomeSeq={3} />);
    expect(sources(worker)).toEqual(requiredSources());
  });
});
