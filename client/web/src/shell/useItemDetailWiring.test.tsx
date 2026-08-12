// @vitest-environment jsdom
//
// #273 added a second writer of an open item's steps that is not this view:
// the cloud runner writes the checklist to the authority, and a terminal
// `ok` asks for one sync cycle. Without a per-cycle re-read, the run would
// report success and the checklist would stay empty until the item was
// closed and reopened — UI state with no reader, the exact failure
// `src/test/component.tsx`'s header exists for.

import { describe, expect, it, vi } from "vitest";
import { render } from "../test/component";
import type { WorkerLike } from "../store/worker-client";
import { useItemDetailWiring } from "./useItemDetailWiring";

function fakeWorker(): WorkerLike & { postMessage: ReturnType<typeof vi.fn> } {
  return { onmessage: null, postMessage: vi.fn() };
}

/** The item is opened on the first render, so the assertions below count
 * sync cycles rather than clicks. `openItemId: null` leaves it closed. */
function Harness({
  worker,
  syncOutcomeSeq,
  openItemId = "item-1",
}: {
  worker: WorkerLike;
  syncOutcomeSeq: number;
  openItemId?: string | null;
}) {
  const { selectedItemId, openItem } = useItemDetailWiring(worker, syncOutcomeSeq);
  if (openItemId !== null && selectedItemId === null) openItem(openItemId);
  return <span>{selectedItemId ?? "none"}</span>;
}

function stepRequests(worker: ReturnType<typeof fakeWorker>): unknown[] {
  return worker.postMessage.mock.calls
    .map(([message]) => message)
    .filter((message) => message.type === "getSteps");
}

describe("useItemDetailWiring", () => {
  it("a bumped syncOutcomeSeq re-issues the steps request for the open item", () => {
    const worker = fakeWorker();
    const { rerender } = render(<Harness worker={worker} syncOutcomeSeq={0} />);
    expect(stepRequests(worker)).toHaveLength(1);

    rerender(<Harness worker={worker} syncOutcomeSeq={1} />);
    expect(stepRequests(worker)).toHaveLength(2);
    expect(stepRequests(worker)[1]).toEqual({ type: "getSteps", itemId: "item-1" });

    // A re-render on the same cycle asks for nothing further.
    rerender(<Harness worker={worker} syncOutcomeSeq={1} />);
    expect(stepRequests(worker)).toHaveLength(2);
  });

  it("no item open means no steps request, however many cycles pass", () => {
    const worker = fakeWorker();
    const { rerender } = render(<Harness worker={worker} syncOutcomeSeq={0} openItemId={null} />);
    rerender(<Harness worker={worker} syncOutcomeSeq={1} openItemId={null} />);
    rerender(<Harness worker={worker} syncOutcomeSeq={2} openItemId={null} />);
    expect(stepRequests(worker)).toHaveLength(0);
  });
});
