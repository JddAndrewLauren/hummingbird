// @vitest-environment jsdom
//
// #273 added a second writer of an open item's steps that is not this view:
// the cloud runner writes the checklist to the authority, and a terminal
// `ok` asks for one sync cycle. Without a per-cycle re-read, the run would
// report success and the checklist would stay empty until the item was
// closed and reopened — UI state with no reader, the exact failure
// `src/test/component.tsx`'s header exists for.

import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "../test/component";
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

  it("opening the item already open closes it — the card is the toggle", () => {
    // The gesture a reader tries first to put an expanded card away, and the
    // one the triage rows have always had.
    const worker = fakeWorker();
    function Toggle() {
      const { selectedItemId, openItem } = useItemDetailWiring(worker, 0);
      return (
        <button type="button" onClick={() => openItem("item-1")}>
          {selectedItemId ?? "none"}
        </button>
      );
    }
    render(<Toggle />);
    const button = () => screen.getByRole("button");

    expect(button().textContent).toBe("none");
    fireEvent.click(button());
    expect(button().textContent).toBe("item-1");
    fireEvent.click(button());
    expect(button().textContent).toBe("none");
  });

  it("opening a different item switches rather than closing", () => {
    const worker = fakeWorker();
    function Two() {
      const { selectedItemId, openItem } = useItemDetailWiring(worker, 0);
      return (
        <>
          <button type="button" onClick={() => openItem("item-1")}>
            one
          </button>
          <button type="button" onClick={() => openItem("item-2")}>
            two
          </button>
          <span>{selectedItemId ?? "none"}</span>
        </>
      );
    }
    render(<Two />);

    fireEvent.click(screen.getByRole("button", { name: "one" }));
    fireEvent.click(screen.getByRole("button", { name: "two" }));
    expect(screen.getByText("item-2")).toBeTruthy();
  });

  it("no item open means no steps request, however many cycles pass", () => {
    const worker = fakeWorker();
    const { rerender } = render(<Harness worker={worker} syncOutcomeSeq={0} openItemId={null} />);
    rerender(<Harness worker={worker} syncOutcomeSeq={1} openItemId={null} />);
    rerender(<Harness worker={worker} syncOutcomeSeq={2} openItemId={null} />);
    expect(stepRequests(worker)).toHaveLength(0);
  });
});
