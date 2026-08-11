// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import type { CoreStatus } from "../store/store";
import type { WorkerLike } from "../store/worker-client";
import { render } from "../test/component";
import { type CalendarEventsRequest, useCalendarEventsWiring } from "./useCalendarEventsWiring";

// Mounted rather than called: a hook that is exported, unit-tested and never
// wired compiles clean and does nothing, which is the failure this app's
// component tests exist to catch (same rationale `usePaneReadsWiring.test.tsx`
// documents for its own hook).

function fakeWorker(): WorkerLike & { postMessage: ReturnType<typeof vi.fn> } {
  return { onmessage: null, postMessage: vi.fn() };
}

function Probe({
  worker,
  status,
  syncOutcomeSeq,
  requests,
}: {
  worker: WorkerLike;
  status: CoreStatus;
  syncOutcomeSeq: number;
  requests: readonly CalendarEventsRequest[];
}) {
  useCalendarEventsWiring(worker, status, syncOutcomeSeq, requests);
  return null;
}

function requestedKeys(worker: ReturnType<typeof fakeWorker>): string[] {
  return worker.postMessage.mock.calls
    .map(([message]) => message as { type: string; key?: string })
    .filter((message) => message.type === "getCalendarEvents")
    .map((message) => message.key ?? "");
}

const WEEKEND: CalendarEventsRequest = { key: "weekend", startMs: 1_000, endMs: 2_000 };

describe("useCalendarEventsWiring", () => {
  it("asks nothing while the core is still loading", () => {
    const worker = fakeWorker();
    render(<Probe worker={worker} status="loading" syncOutcomeSeq={0} requests={[WEEKEND]} />);
    expect(worker.postMessage).not.toHaveBeenCalled();
  });

  it("asks nothing when the caller has no requests to make", () => {
    // #267 builds the seam, not a standing question — an empty request list
    // is the correct steady state before any pane registers one.
    const worker = fakeWorker();
    render(<Probe worker={worker} status="ready" syncOutcomeSeq={0} requests={[]} />);
    expect(worker.postMessage).not.toHaveBeenCalled();
  });

  it("requests every caller-supplied interval once the core is ready", () => {
    const worker = fakeWorker();
    render(<Probe worker={worker} status="ready" syncOutcomeSeq={0} requests={[WEEKEND]} />);
    expect(requestedKeys(worker)).toEqual(["weekend"]);
  });

  it("re-requests on every completed cycle", () => {
    const worker = fakeWorker();
    const view = render(
      <Probe worker={worker} status="ready" syncOutcomeSeq={0} requests={[WEEKEND]} />,
    );
    view.rerender(<Probe worker={worker} status="ready" syncOutcomeSeq={1} requests={[WEEKEND]} />);
    expect(requestedKeys(worker)).toEqual(["weekend", "weekend"]);
  });

  it("does not re-request on a render that changed nothing, even with a fresh array reference", () => {
    const worker = fakeWorker();
    const view = render(
      <Probe worker={worker} status="ready" syncOutcomeSeq={3} requests={[WEEKEND]} />,
    );
    // A brand-new array with an equal value — the common case for a caller
    // deriving the interval fresh each render.
    view.rerender(
      <Probe
        worker={worker}
        status="ready"
        syncOutcomeSeq={3}
        requests={[{ key: "weekend", startMs: 1_000, endMs: 2_000 }]}
      />,
    );
    expect(requestedKeys(worker)).toEqual(["weekend"]);
  });

  it("adds no interval or timeout of its own", () => {
    const setIntervalSpy = vi.spyOn(window, "setInterval");
    const setTimeoutSpy = vi.spyOn(window, "setTimeout");
    const worker = fakeWorker();
    render(<Probe worker={worker} status="ready" syncOutcomeSeq={0} requests={[WEEKEND]} />);
    expect(setIntervalSpy).not.toHaveBeenCalled();
    expect(setTimeoutSpy).not.toHaveBeenCalled();
    setIntervalSpy.mockRestore();
    setTimeoutSpy.mockRestore();
  });
});
