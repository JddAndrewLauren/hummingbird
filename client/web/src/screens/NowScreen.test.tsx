// @vitest-environment jsdom

// The regression suite for PR #207's round-2 defect, and the reason this
// repo now has component tests at all.
//
// The defect: `applyItemAction` freezes `pending: true` onto the optimistic
// item. `"block"` moves an item to `Stage::Blocked`, which is outside BOTH
// `getFrontier` and `getBlocked` (S10's scope), so nothing ever replaced
// that frozen snapshot — the detail panel's Start/Cancel row rendered
// `disabled` forever and the item became functionally unreachable. It
// survived TWO review rounds because every piece of it was individually
// correct: `resolveFallbackPending` is unit-tested and right, `ItemRow` is
// right, `ItemDetailPanel` is right. Only the thread between them was
// broken, and a thread is exactly what typecheck cannot see.
//
// So these tests drive the whole lifecycle through the mounted screen: act,
// watch the item leave every live query, then walk `TaskState.pending` the
// way `worker-client.ts` really walks it, and assert the row comes back.

import { describe, expect, it, vi } from "vitest";
import { NowScreen } from "./NowScreen";
import {
  blockedEntryDTO,
  fireEvent,
  itemDTO,
  projectDTO,
  render,
  screen,
  taskState,
} from "../test/component";
import type { TaskState } from "../store/store";

const NOW_MS = 1_700_000_000_000;

function renderNow(task: TaskState, selectedItemId: string | null = null) {
  const onAct = vi.fn();
  const onOpenItem = vi.fn();
  const onCloseItemDetail = vi.fn();
  const view = render(
    <NowScreen
      demo={null}
      tile={null}
      onScreen={() => {}}
      task={task}
      nowMs={NOW_MS}
      selectedItemId={selectedItemId}
      onOpenItem={onOpenItem}
      onCloseItemDetail={onCloseItemDetail}
      onAct={onAct}
    />,
  );
  const rerender = (next: TaskState, nextSelected: string | null = selectedItemId) =>
    view.rerender(
      <NowScreen
        demo={null}
        tile={null}
        onScreen={() => {}}
        task={next}
        nowMs={NOW_MS}
        selectedItemId={nextSelected}
        onOpenItem={onOpenItem}
        onCloseItemDetail={onCloseItemDetail}
        onAct={onAct}
      />,
    );
  return { onAct, onOpenItem, onCloseItemDetail, rerender };
}

describe("NowScreen — the act lifecycle (PR #207 round 2)", () => {
  it("re-enables the action row once the queued act drains, for an item that left every live query", () => {
    const item = itemDTO({ id: "i1", title: "Renew the passport", stage: "ready" });
    const { onAct, rerender } = renderNow(taskState({ frontier: [item] }), "i1");

    // Ready offers start / block / cancel.
    const blockButton = screen.getByRole("button", { name: /mark blocked/i });
    fireEvent.click(blockButton);
    expect(onAct).toHaveBeenCalledWith("i1", "block");

    // The mutation is queued: the item is now Stage::Blocked, which neither
    // getFrontier nor getBlocked returns. The panel must stay open on the
    // optimistic projection rather than going blank.
    rerender(taskState({ frontier: [], blocked: [], pending: {} }));
    expect(screen.getByRole("heading", { name: "Renew the passport" })).toBeDefined();
    const startWhileQueued = screen.getByRole("button", { name: /^start$/i });
    expect(startWhileQueued.hasAttribute("disabled")).toBe(true);

    // `worker-client.ts` re-reads isPending on the ok actResult: still true.
    rerender(taskState({ frontier: [], blocked: [], pending: { i1: true } }));
    expect(screen.getByRole("button", { name: /^start$/i }).hasAttribute("disabled")).toBe(true);

    // The cycle drains it. THIS is the assertion the defect failed: the
    // frozen optimistic `pending: true` must not outlive the live read.
    rerender(taskState({ frontier: [], blocked: [], pending: { i1: false } }));
    expect(screen.getByRole("button", { name: /^start$/i }).hasAttribute("disabled")).toBe(false);
  });

  it("does not re-enable the row between two acts, while the second is still unconfirmed", () => {
    const item = itemDTO({ id: "i1", stage: "ready" });
    const { rerender } = renderNow(taskState({ frontier: [item] }), "i1");

    fireEvent.click(screen.getByRole("button", { name: /mark blocked/i }));
    rerender(taskState({ frontier: [], pending: { i1: true } }));
    rerender(taskState({ frontier: [], pending: { i1: false } }));

    // Second act, fired while the store still holds the FIRST act's drained
    // `false` — the row must disable immediately, not flicker enabled.
    // Starting a blocked item projects it to `in_progress`, so the row it
    // re-renders as is that stage's own (complete / block / cancel).
    fireEvent.click(screen.getByRole("button", { name: /^start$/i }));
    rerender(taskState({ frontier: [], pending: { i1: false } }));
    expect(screen.getByRole("button", { name: /complete/i }).hasAttribute("disabled")).toBe(true);

    // ...and it stays disabled until this act's own `true` has been seen.
    rerender(taskState({ frontier: [], pending: { i1: true } }));
    expect(screen.getByRole("button", { name: /complete/i }).hasAttribute("disabled")).toBe(true);
    rerender(taskState({ frontier: [], pending: { i1: false } }));
    expect(screen.getByRole("button", { name: /complete/i }).hasAttribute("disabled")).toBe(false);
  });

  it("drops a previous selection's optimistic item when a different item opens", () => {
    const one = itemDTO({ id: "i1", title: "First", stage: "ready" });
    const two = itemDTO({ id: "i2", title: "Second", stage: "ready" });
    const { rerender } = renderNow(taskState({ frontier: [one, two] }), "i1");

    fireEvent.click(screen.getByRole("button", { name: /mark blocked/i }));
    // i1 has left the frontier; i2 is opened instead.
    rerender(taskState({ frontier: [two], pending: { i1: true } }), "i2");

    expect(screen.getByRole("heading", { name: "Second" })).toBeDefined();
    expect(screen.queryByRole("heading", { name: "First" })).toBeNull();
    // i2 is live and unqueued, so its row is usable — the stale optimistic
    // `pending: true` from i1 must not have leaked across.
    expect(screen.getByRole("button", { name: /^start$/i }).hasAttribute("disabled")).toBe(false);
  });

  it("surfaces a failed act, and only for the item it belongs to", () => {
    const item = itemDTO({ id: "i1", stage: "ready" });
    const other = itemDTO({ id: "i2", stage: "ready" });
    const failure = {
      seed: "s",
      itemId: "i2",
      action: "start" as const,
      kind: "failed" as const,
      error: "Nope.",
    };
    const { rerender } = renderNow(
      taskState({ frontier: [item, other], lastAct: failure }),
      "i1",
    );
    // The failure belongs to i2; i1's panel must not wear it.
    expect(screen.queryByText("Nope.")).toBeNull();

    rerender(taskState({ frontier: [item, other], lastAct: failure }), "i2");
    expect(screen.getByText("Nope.")).toBeDefined();
  });
});

describe("NowScreen — the frontier list", () => {
  it("marks a pending item as such, and leaves a confirmed one unmarked", () => {
    // Issue #108's acceptance criterion, which shipped dead once already
    // (PR #200: `requestIsPending` had no caller).
    renderNow(
      taskState({
        frontier: [
          itemDTO({ id: "i1", title: "Queued offline", pending: true }),
          itemDTO({ id: "i2", title: "Confirmed", pending: false }),
        ],
      }),
    );
    expect(screen.getAllByText("Pending")).toHaveLength(1);
  });

  it("groups by project name, with the unassigned group last", () => {
    renderNow(
      taskState({
        frontier: [
          itemDTO({ id: "i1", title: "Loose", projectId: null }),
          itemDTO({ id: "i2", title: "Owned", projectId: "p1" }),
        ],
        projects: [projectDTO({ id: "p1", name: "Kitchen rebuild" })],
      }),
    );
    const headings = screen.getAllByRole("heading").map((node) => node.textContent);
    expect(headings.indexOf("Kitchen rebuild")).toBeGreaterThanOrEqual(0);
    expect(headings.indexOf("Kitchen rebuild")).toBeLessThan(headings.indexOf("No project"));
  });

  it("names the blockers holding an item off the frontier", () => {
    renderNow(
      taskState({
        blocked: [
          blockedEntryDTO(itemDTO({ id: "i1", title: "Hang the door" }), [
            itemDTO({ id: "b1", title: "Buy hinges" }),
          ]),
        ],
      }),
    );
    expect(screen.getByText(/Buy hinges/)).toBeDefined();
  });

  it("opens item detail on a frontier row click", () => {
    const { onOpenItem } = renderNow(
      taskState({ frontier: [itemDTO({ id: "i1", title: "Renew the passport" })] }),
    );
    fireEvent.click(screen.getByRole("button", { name: /Renew the passport/ }));
    expect(onOpenItem).toHaveBeenCalledWith("i1");
  });

  it("says nothing is startable when both queries are empty", () => {
    renderNow(taskState());
    expect(screen.getByText("Nothing to start")).toBeDefined();
  });
});
