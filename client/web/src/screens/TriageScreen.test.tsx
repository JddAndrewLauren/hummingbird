// @vitest-environment jsdom

// The regression suite for the triage threading (#111 / PR #213), rewritten
// when the inbox became one collapsed line per capture with the editor behind a
// selection. The capture box's own thread moved with the box itself, into
// `shell/CapturePopover.test.tsx`.
//
// The pure modules under this screen are all individually tested:
// `buildTriageEdits` diffs a draft against its item, `triageDraftProblems`
// names what cannot be sent, `orderTriage` sorts by capture order. What no node
// test could reach is whether the screen actually CALLS them — whether a row
// really starts collapsed, whether promoting really sends ONE `onTriage`
// carrying every changed field rather than one call per field, whether an
// invalid field really stops the promotion. That thread is what these mount.

import { describe, expect, it, vi } from "vitest";
import { TriageScreen } from "./TriageScreen";
import { fireEvent, itemDTO, projectDTO, render, screen, taskState, within } from "../test/component";
import type { TaskState } from "../store/store";

const NOW = 10 * 60 * 60 * 1000;

function renderTriage(task: TaskState, options: { withTriage?: boolean } = {}) {
  const onTriage = vi.fn();
  const view = render(
    <TriageScreen
      demo={null}
      task={task}
      onTriage={options.withTriage === false ? undefined : onTriage}
      nowMs={NOW}
    />,
  );
  const rerender = (nextTask: TaskState) =>
    view.rerender(
      <TriageScreen
        demo={null}
        task={nextTask}
        onTriage={options.withTriage === false ? undefined : onTriage}
        nowMs={NOW}
      />,
    );
  return { onTriage, rerender };
}

/** The collapsed row IS the button that expands it, and its accessible name is
 * the whole line, so it is reached by the title inside it. */
function row(title: string): HTMLElement {
  return screen.getByRole("button", { name: new RegExp(title, "i") });
}

/** The open editor, reached from its own Promote button: that button sits in the
 * editor's button strip, whose parent is the element holding every field.
 * Structural, but it is the only handle the component offers without adding a
 * test-only attribute to production markup — and it keeps `getByLabelText`
 * unambiguous when several rows are on screen. */
function editor(): HTMLElement {
  const promote = screen.getByRole("button", { name: /promote to ready/i });
  const form = promote.parentElement?.parentElement;
  if (!form) {
    throw new Error("triage row editor not found — the row's markup changed");
  }
  return form;
}

function field(label: string): HTMLElement {
  return within(editor()).getByLabelText(label);
}

describe("TriageScreen — the collapsed inbox", () => {
  it("renders one line per capture, with no editor open", () => {
    renderTriage(
      taskState({
        triageInbox: [
          itemDTO({ id: "i1", title: "ask dad about the trailer hitch", createdAt: NOW - 7_200_000 }),
          itemDTO({ id: "i2", title: "the fence gate is dragging again", createdAt: NOW }),
        ],
      }),
    );

    expect(screen.getByText("ask dad about the trailer hitch")).toBeDefined();
    expect(screen.getByText("the fence gate is dragging again")).toBeDefined();
    expect(screen.queryByRole("button", { name: /promote to ready/i })).toBeNull();
    expect(screen.queryByLabelText("Due date")).toBeNull();
  });

  it("states each capture's provenance and age on its line", () => {
    renderTriage(
      taskState({
        triageInbox: [
          itemDTO({
            id: "i1",
            title: "swept in",
            source: "google-tasks/v1",
            createdAt: NOW - 7_200_000,
          }),
          itemDTO({ id: "i2", title: "typed in", source: null, createdAt: NOW - 60_000 }),
        ],
      }),
    );

    expect(screen.getByText(/google-tasks\/v1 · 2h ago/)).toBeDefined();
    // No source is reported as no source, never as a fabricated one.
    expect(screen.getByText(/typed here · 1m ago/)).toBeDefined();
  });

  it("expands the row that is selected, and only that one", () => {
    renderTriage(
      taskState({
        triageInbox: [
          itemDTO({ id: "i1", title: "first thing" }),
          itemDTO({ id: "i2", title: "second thing" }),
        ],
      }),
    );

    fireEvent.click(row("first thing"));
    expect(row("first thing").getAttribute("aria-expanded")).toBe("true");
    expect(row("second thing").getAttribute("aria-expanded")).toBe("false");
    expect(screen.getAllByRole("button", { name: /promote to ready/i })).toHaveLength(1);

    // Selecting another row moves the selection rather than opening a second
    // editor with its own unsent draft.
    fireEvent.click(row("second thing"));
    expect(row("first thing").getAttribute("aria-expanded")).toBe("false");
    expect(row("second thing").getAttribute("aria-expanded")).toBe("true");
    expect(screen.getAllByRole("button", { name: /promote to ready/i })).toHaveLength(1);

    // And clicking the open one closes it — an inbox at rest is a list.
    fireEvent.click(row("second thing"));
    expect(screen.queryByRole("button", { name: /promote to ready/i })).toBeNull();
  });

  it("expands into no editor at all without an onTriage handler", () => {
    renderTriage(taskState({ triageInbox: [itemDTO({ id: "i1", title: "vague thing" })] }), {
      withTriage: false,
    });
    fireEvent.click(row("vague thing"));
    expect(screen.queryByRole("button", { name: /promote to ready/i })).toBeNull();
  });

  it("keeps the draft after promoting, until the result comes back ok", () => {
    // Issue #222: the draft used to clear the instant Promote was clicked,
    // optimistically — this is what makes a failed triage lose the reader's
    // edits on top of saying nothing about the failure.
    const { onTriage, rerender } = renderTriage(
      taskState({ triageInbox: [itemDTO({ id: "i1", title: "vague thing" })] }),
    );
    fireEvent.click(row("vague thing"));
    fireEvent.change(field("Title"), { target: { value: "Order the worktop" } });
    fireEvent.click(screen.getByRole("button", { name: /promote to ready/i }));
    expect(onTriage).toHaveBeenCalledTimes(1);
    // Still in flight — no result has come back yet.
    expect((field("Title") as HTMLInputElement).value).toBe("Order the worktop");

    rerender(
      taskState({
        triageInbox: [itemDTO({ id: "i1", title: "vague thing" })],
        lastTriage: { seed: "s1", itemId: "i1", kind: "ok", error: null },
      }),
    );
    // The typing cleared: the field shows the item's own value again —
    // `effectiveDraft` seeds from the item, never from blank.
    expect((field("Title") as HTMLInputElement).value).toBe("vague thing");
  });

  it("surfaces a failed triage and leaves the draft in place", () => {
    const { onTriage, rerender } = renderTriage(
      taskState({ triageInbox: [itemDTO({ id: "i1", title: "vague thing" })] }),
    );
    fireEvent.click(row("vague thing"));
    fireEvent.change(field("Title"), { target: { value: "Order the worktop" } });
    fireEvent.click(screen.getByRole("button", { name: /promote to ready/i }));
    expect(onTriage).toHaveBeenCalledTimes(1);

    rerender(
      taskState({
        triageInbox: [itemDTO({ id: "i1", title: "vague thing" })],
        lastTriage: { seed: "s1", itemId: "i1", kind: "failed", error: "Nope." },
      }),
    );
    expect(screen.getByText("Nope.")).toBeDefined();
    // The edits are still here to retry or amend.
    expect((field("Title") as HTMLInputElement).value).toBe("Order the worktop");
  });

  it("announces a failed triage to a screen reader, even on a collapsed row", () => {
    renderTriage(
      taskState({
        triageInbox: [itemDTO({ id: "i1", title: "vague thing" })],
        lastTriage: { seed: "s1", itemId: "i1", kind: "failed", error: "Nope." },
      }),
    );
    expect(screen.getByRole("alert").textContent).toBe("Nope.");
  });

  it("does not wear a failure that belongs to a different item", () => {
    renderTriage(
      taskState({
        triageInbox: [itemDTO({ id: "i1", title: "vague thing" })],
        lastTriage: { seed: "s1", itemId: "i2", kind: "failed", error: "Nope." },
      }),
    );
    expect(screen.queryByText("Nope.")).toBeNull();
  });

  it("says the inbox is empty when it is", () => {
    renderTriage(taskState());
    expect(screen.getByText("Triage is empty")).toBeDefined();
  });
});

describe("TriageScreen — the editor", () => {
  it("shows the item's own values rather than blank fields", () => {
    renderTriage(
      taskState({
        triageInbox: [
          itemDTO({
            id: "i1",
            title: "Order the worktop",
            description: "oak, 3m",
            size: "deep",
            energy: "high",
            context: "@computer",
            priority: 2,
            projectId: "p1",
            deadline: "2026-08-14",
            scheduledDate: "2026-08-12",
          }),
        ],
        projects: [projectDTO({ id: "p1", name: "Kitchen rebuild" })],
      }),
    );
    fireEvent.click(row("Order the worktop"));

    expect((field("Title") as HTMLInputElement).value).toBe("Order the worktop");
    expect((field("Description") as HTMLTextAreaElement).value).toBe("oak, 3m");
    expect((field("Project") as HTMLSelectElement).value).toBe("p1");
    expect((field("Priority") as HTMLSelectElement).value).toBe("2");
    expect((field("Size") as HTMLSelectElement).value).toBe("deep");
    expect((field("Energy") as HTMLSelectElement).value).toBe("high");
    expect((field("Context") as HTMLSelectElement).value).toBe("@computer");
    expect((field("Due date") as HTMLInputElement).value).toBe("2026-08-14");
    expect((field("Scheduled date") as HTMLInputElement).value).toBe("2026-08-12");
  });

  it("sends a multi-field edit as exactly ONE call carrying every changed field", () => {
    // #111's acceptance criterion. One call, not one per field — a 409 must
    // rebase or dead-letter the whole edit together.
    const { onTriage } = renderTriage(
      taskState({
        triageInbox: [itemDTO({ id: "i1", title: "vague thing" })],
        projects: [projectDTO({ id: "p1", name: "Kitchen rebuild" })],
      }),
    );
    fireEvent.click(row("vague thing"));

    fireEvent.change(field("Title"), { target: { value: "Order the worktop" } });
    fireEvent.change(field("Description"), { target: { value: "oak, 3m" } });
    fireEvent.change(field("Project"), { target: { value: "p1" } });
    fireEvent.change(field("Priority"), { target: { value: "2" } });
    fireEvent.change(field("Size"), { target: { value: "deep" } });
    fireEvent.change(field("Energy"), { target: { value: "high" } });
    fireEvent.change(field("Context"), { target: { value: "@computer" } });
    fireEvent.change(field("Due date"), { target: { value: "2026-08-14" } });
    fireEvent.change(field("Scheduled date"), { target: { value: "2026-08-12" } });
    fireEvent.click(screen.getByRole("button", { name: /promote to ready/i }));

    expect(onTriage).toHaveBeenCalledTimes(1);
    expect(onTriage).toHaveBeenCalledWith("i1", "ready", {
      title: "Order the worktop",
      description: "oak, 3m",
      projectId: "p1",
      priority: 2,
      size: "deep",
      energy: "high",
      context: "@computer",
      deadline: "2026-08-14",
      scheduledDate: "2026-08-12",
    });
  });

  it("sends nothing but the promotion when nothing was edited", () => {
    const { onTriage } = renderTriage(
      taskState({ triageInbox: [itemDTO({ id: "i1", title: "vague thing" })] }),
    );
    fireEvent.click(row("vague thing"));
    fireEvent.click(screen.getByRole("button", { name: /send to grilling/i }));
    expect(onTriage).toHaveBeenCalledWith("i1", "grilling", {});
  });

  it("clears a field that is emptied, rather than leaving it alone", () => {
    const { onTriage } = renderTriage(
      taskState({
        triageInbox: [
          itemDTO({ id: "i1", title: "vague thing", context: "@computer", deadline: "2026-08-14" }),
        ],
      }),
    );
    fireEvent.click(row("vague thing"));

    fireEvent.change(field("Context"), { target: { value: "" } });
    fireEvent.change(field("Due date"), { target: { value: "" } });
    fireEvent.click(screen.getByRole("button", { name: /promote to ready/i }));

    expect(onTriage).toHaveBeenCalledWith("i1", "ready", { context: null, deadline: null });
  });

  it("refuses to promote while a field cannot be sent, and says which", () => {
    const { onTriage } = renderTriage(
      taskState({ triageInbox: [itemDTO({ id: "i1", title: "vague thing" })] }),
    );
    fireEvent.click(row("vague thing"));

    fireEvent.change(field("Due date"), { target: { value: "14/08/2026" } });
    expect(within(editor()).getByText(/YYYY-MM-DD/)).toBeDefined();
    expect(
      screen.getByRole("button", { name: /promote to ready/i }).hasAttribute("disabled"),
    ).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: /promote to ready/i }));
    expect(onTriage).not.toHaveBeenCalled();

    // Fixing it re-enables the promotion — the block is the field's current
    // state, not a latch.
    fireEvent.change(field("Due date"), { target: { value: "2026-08-14" } });
    expect(
      screen.getByRole("button", { name: /promote to ready/i }).hasAttribute("disabled"),
    ).toBe(false);
  });

  it("marks a pending capture and refuses to promote it", () => {
    renderTriage(
      taskState({ triageInbox: [itemDTO({ id: "i1", title: "Queued offline", pending: true })] }),
    );
    expect(screen.getByText("Pending")).toBeDefined();
    fireEvent.click(row("Queued offline"));
    expect(
      screen.getByRole("button", { name: /promote to ready/i }).hasAttribute("disabled"),
    ).toBe(true);
  });
});

// The row checkmark: `Core::act`'s complete offered straight off the
// collapsed line — the recorded amendment to "Triage is pre-action by
// definition". What only a mount can prove is the sibling structure: the
// checkmark must fire WITHOUT toggling the editor open, and must not exist
// at all when no handler is wired (demo mode).
describe("TriageScreen — the mark-done checkmark", () => {
  function renderWithComplete(task: TaskState) {
    const onComplete = vi.fn();
    render(
      <TriageScreen demo={null} task={task} onTriage={vi.fn()} onComplete={onComplete} nowMs={NOW} />,
    );
    return { onComplete };
  }

  it("completes the capture in one click, without expanding the editor", () => {
    const { onComplete } = renderWithComplete(
      taskState({ triageInbox: [itemDTO({ id: "i1", title: "already did this" })] }),
    );

    fireEvent.click(screen.getByRole("button", { name: 'Mark "already did this" done' }));
    expect(onComplete).toHaveBeenCalledWith("i1");
    // The editor stayed shut: completing is not a selection.
    expect(screen.queryByRole("button", { name: /promote to ready/i })).toBeNull();
  });

  it("disables the checkmark while the row is pending", () => {
    renderWithComplete(
      taskState({ triageInbox: [itemDTO({ id: "i1", title: "queued thing", pending: true })] }),
    );
    expect(
      screen.getByRole("button", { name: 'Mark "queued thing" done' }).hasAttribute("disabled"),
    ).toBe(true);
  });

  it("offers no checkmark without an onComplete handler", () => {
    renderTriage(taskState({ triageInbox: [itemDTO({ id: "i1", title: "demo-ish row" })] }));
    expect(screen.queryByRole("button", { name: /mark .* done/i })).toBeNull();
  });
});
