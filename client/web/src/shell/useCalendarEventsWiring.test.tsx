// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import * as registry from "../screens/questions/registry";
import type { CoreStatus } from "../store/store";
import type { WorkerLike } from "../store/worker-client";
import { render } from "../test/component";
import { useCalendarEventsWiring } from "./useCalendarEventsWiring";

// Mounted rather than called: a hook that is exported, unit-tested and never
// wired compiles clean and does nothing, which is the failure this app's
// component tests exist to catch (same rationale `usePaneReadsWiring.test.tsx`
// documents for its own hook). Requests are read from the registry, not a
// prop — `requiredCalendarRequests` is stubbed per test so the general
// request/re-request behaviour stays exercised independent of exactly which
// questions are registered; one test below (deliberately unmocked) pins the
// real, registered steady state (#122's weekend-plans pane).

function fakeWorker(): WorkerLike & { postMessage: ReturnType<typeof vi.fn> } {
  return { onmessage: null, postMessage: vi.fn() };
}

function Probe({ worker, status, syncOutcomeSeq }: {
  worker: WorkerLike;
  status: CoreStatus;
  syncOutcomeSeq: number;
}) {
  useCalendarEventsWiring(worker, status, syncOutcomeSeq);
  return null;
}

function requestedKeys(worker: ReturnType<typeof fakeWorker>): string[] {
  return worker.postMessage.mock.calls
    .map(([message]) => message as { type: string; key?: string })
    .filter((message) => message.type === "getCalendarEvents")
    .map((message) => message.key ?? "");
}

const WEEKEND = {
  key: "weekend",
  startMs: 1_000,
  endMs: 2_000,
  startDate: "2026-08-14",
  endDate: "2026-08-17",
};

describe("useCalendarEventsWiring", () => {
  it("asks nothing while the core is still loading", () => {
    vi.spyOn(registry, "requiredCalendarRequests").mockReturnValue([WEEKEND]);
    const worker = fakeWorker();
    render(<Probe worker={worker} status="loading" syncOutcomeSeq={0} />);
    expect(worker.postMessage).not.toHaveBeenCalled();
    vi.restoreAllMocks();
  });

  it("asks for every registered pane's own interval — today's real, registered steady state (#122, #121, #693)", () => {
    // No mock: exercises the actual `requiredCalendarRequests()`, which
    // answers every calendar-lane pane's interval, in `QUESTION_ORDER`.
    const worker = fakeWorker();
    render(<Probe worker={worker} status="ready" syncOutcomeSeq={0} />);
    expect(requestedKeys(worker)).toEqual(["scps", "weekend", "vacation"]);
  });

  it("carries the civil bounds alongside the millisecond ones", () => {
    // The all-day arm can only be asked about in dates (ADR-0015's
    // 2026-08-10 amendment), so a request that dropped them would answer
    // every all-day question with silence — and the request would still
    // look right in every other respect.
    const worker = fakeWorker();
    render(<Probe worker={worker} status="ready" syncOutcomeSeq={0} />);
    const request = worker.postMessage.mock.calls
      .map(([message]) => message as Record<string, unknown>)
      .find((message) => message.type === "getCalendarEvents");
    expect(request?.startDate).toMatch(/^\d{4}-\d{2}-\d{2}$/);
    expect(request?.endDate).toMatch(/^\d{4}-\d{2}-\d{2}$/);
    // Exclusive end, three days wide: Friday through the Monday after.
    expect(String(request?.startDate) < String(request?.endDate)).toBe(true);
  });

  it("requests every interval the registry declares once the core is ready", () => {
    vi.spyOn(registry, "requiredCalendarRequests").mockReturnValue([WEEKEND]);
    const worker = fakeWorker();
    render(<Probe worker={worker} status="ready" syncOutcomeSeq={0} />);
    expect(requestedKeys(worker)).toEqual(["weekend"]);
    vi.restoreAllMocks();
  });

  it("re-requests on every completed cycle", () => {
    vi.spyOn(registry, "requiredCalendarRequests").mockReturnValue([WEEKEND]);
    const worker = fakeWorker();
    const view = render(<Probe worker={worker} status="ready" syncOutcomeSeq={0} />);
    view.rerender(<Probe worker={worker} status="ready" syncOutcomeSeq={1} />);
    expect(requestedKeys(worker)).toEqual(["weekend", "weekend"]);
    vi.restoreAllMocks();
  });

  it("does not re-request on a render that changed nothing, even with a fresh array reference each call", () => {
    // A brand-new array with an equal value — the common case for a
    // registry deriving the list fresh each call.
    vi.spyOn(registry, "requiredCalendarRequests").mockImplementation(() => [
      {
        key: "weekend",
        startMs: 1_000,
        endMs: 2_000,
        startDate: "2026-08-14",
        endDate: "2026-08-17",
      },
    ]);
    const worker = fakeWorker();
    const view = render(<Probe worker={worker} status="ready" syncOutcomeSeq={3} />);
    view.rerender(<Probe worker={worker} status="ready" syncOutcomeSeq={3} />);
    expect(requestedKeys(worker)).toEqual(["weekend"]);
    vi.restoreAllMocks();
  });

  it("adds no interval or timeout of its own", () => {
    vi.spyOn(registry, "requiredCalendarRequests").mockReturnValue([WEEKEND]);
    const setIntervalSpy = vi.spyOn(window, "setInterval");
    const setTimeoutSpy = vi.spyOn(window, "setTimeout");
    const worker = fakeWorker();
    render(<Probe worker={worker} status="ready" syncOutcomeSeq={0} />);
    expect(setIntervalSpy).not.toHaveBeenCalled();
    expect(setTimeoutSpy).not.toHaveBeenCalled();
    setIntervalSpy.mockRestore();
    setTimeoutSpy.mockRestore();
    vi.restoreAllMocks();
  });
});
