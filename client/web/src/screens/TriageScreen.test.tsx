// @vitest-environment jsdom

// The regression suite for the capture and triage threading (#110 / PR
// #206, #111 / PR #213).
//
// The pure modules under this screen are all individually tested:
// `canSubmitCapture` refuses an empty draft, `buildTriageEdits` maps a blank
// field to `null`, `orderTriage` sorts by capture order. What no node test
// could reach is whether the screen actually CALLS them — whether the Add
// button really consults `canSubmitCapture`, whether promoting really sends
// ONE `onTriage` carrying every drafted field rather than one call per
// field. That thread is what these mount.

import { describe, expect, it, vi } from "vitest";
import { TriageScreen } from "./TriageScreen";
import { fireEvent, itemDTO, projectDTO, render, screen, taskState, within } from "../test/component";
import type { TaskState } from "../store/store";

/** The capture box at the top of this screen owns its own "Size", "Energy"
 * and "Context" controls, so those label texts are not unique on the page
 * and a bare `getByLabelText` is ambiguous. Scope to the one triage row's
 * edit form instead, reached from its own Promote button: that button sits
 * in the row's button strip, whose parent is the form holding every field.
 * Structural, but it is the only handle the component offers without adding
 * a test-only attribute to production markup. */
function rowForm(): HTMLElement {
  const promote = screen.getByRole("button", { name: /promote to ready/i });
  const form = promote.parentElement?.parentElement;
  if (!form) {
    throw new Error("triage row form not found — the row's markup changed");
  }
  return form;
}

function rowField(label: string): HTMLElement {
  return within(rowForm()).getByLabelText(label);
}

function renderTriage(task: TaskState, options: { withTriage?: boolean } = {}) {
  const onSubmitCapture = vi.fn();
  const onTriage = vi.fn();
  const view = render(
    <TriageScreen
      demo={null}
      task={task}
      onSubmitCapture={onSubmitCapture}
      onTriage={options.withTriage === false ? undefined : onTriage}
      focusRequestId={0}
    />,
  );
  const rerender = (nextTask: TaskState) =>
    view.rerender(
      <TriageScreen
        demo={null}
        task={nextTask}
        onSubmitCapture={onSubmitCapture}
        onTriage={options.withTriage === false ? undefined : onTriage}
        focusRequestId={0}
      />,
    );
  return { onSubmitCapture, onTriage, rerender };
}

describe("TriageScreen — the capture box", () => {
  it("refuses an empty or whitespace-only draft, and accepts a real one", () => {
    renderTriage(taskState());
    const button = screen.getByRole("button", { name: /add to triage/i });
    const input = screen.getByLabelText("Capture");

    expect(button.hasAttribute("disabled")).toBe(true);

    fireEvent.change(input, { target: { value: "   " } });
    expect(button.hasAttribute("disabled")).toBe(true);

    fireEvent.change(input, { target: { value: "Call the plumber" } });
    expect(button.hasAttribute("disabled")).toBe(false);
  });

  it("submits the draft, and clears the box once the result comes back ok", () => {
    const { onSubmitCapture, rerender } = renderTriage(taskState());
    const input = screen.getByLabelText("Capture") as HTMLInputElement;

    fireEvent.change(input, { target: { value: "Call the plumber" } });
    fireEvent.click(screen.getByRole("button", { name: /add to triage/i }));

    expect(onSubmitCapture).toHaveBeenCalledTimes(1);
    expect(onSubmitCapture.mock.calls[0][0]).toBe("Call the plumber");

    rerender(taskState({ lastCapture: { seed: "s1", kind: "ok", id: "item-9", error: null } }));
    expect((screen.getByLabelText("Capture") as HTMLInputElement).value).toBe("");
  });

  it("submits on Enter", () => {
    const { onSubmitCapture } = renderTriage(taskState());
    const input = screen.getByLabelText("Capture");
    fireEvent.change(input, { target: { value: "Call the plumber" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(onSubmitCapture).toHaveBeenCalledTimes(1);
  });

  it("does not submit on the Enter that commits an IME composition", () => {
    const { onSubmitCapture } = renderTriage(taskState());
    const input = screen.getByLabelText("Capture");
    fireEvent.change(input, { target: { value: "植物に水をやる" } });
    fireEvent.keyDown(input, { key: "Enter", isComposing: true });
    expect(onSubmitCapture).not.toHaveBeenCalled();
  });

  // #208's headline acceptance, proved from the rendered controls
  // themselves — not just that `resolveCaptureFields` (the pure layer)
  // accepts a `CaptureMeta`. The Energy/Size sliders are `role="slider"`
  // elements moved with the keyboard (`End` jumps to the last stop, per
  // `Slider.tsx`'s own `onKeyDown`), never a plain `<input>`.
  it("carries the capture box's Energy/Size/Context selections onto the wire message", () => {
    const { onSubmitCapture } = renderTriage(taskState());
    fireEvent.change(screen.getByLabelText("Capture"), { target: { value: "Buy soil" } });
    fireEvent.keyDown(screen.getByRole("slider", { name: "Energy" }), { key: "End" });
    fireEvent.keyDown(screen.getByRole("slider", { name: "Size" }), { key: "End" });
    fireEvent.change(screen.getByLabelText("Context"), { target: { value: "@garden" } });

    fireEvent.click(screen.getByRole("button", { name: /add to triage/i }));

    expect(onSubmitCapture).toHaveBeenCalledWith("Buy soil", expect.any(Number), {
      size: "deep",
      energy: "high",
      context: "@garden",
    });
  });

  it("leaves size, energy and context absent when the controls are left at rest", () => {
    const { onSubmitCapture } = renderTriage(taskState());
    fireEvent.change(screen.getByLabelText("Capture"), { target: { value: "Buy soil" } });

    fireEvent.click(screen.getByRole("button", { name: /add to triage/i }));

    expect(onSubmitCapture).toHaveBeenCalledWith("Buy soil", expect.any(Number), {
      size: null,
      energy: null,
      context: null,
    });
  });

  it("sends only the one field the reader set, leaving the other two absent", () => {
    const { onSubmitCapture } = renderTriage(taskState());
    fireEvent.change(screen.getByLabelText("Capture"), { target: { value: "Buy soil" } });
    fireEvent.keyDown(screen.getByRole("slider", { name: "Energy" }), { key: "End" });

    fireEvent.click(screen.getByRole("button", { name: /add to triage/i }));

    expect(onSubmitCapture).toHaveBeenCalledWith("Buy soil", expect.any(Number), {
      size: null,
      energy: "high",
      context: null,
    });
  });

  it("clears the Energy/Size/Context controls back to rest on an ok result", () => {
    const { rerender } = renderTriage(taskState());
    fireEvent.change(screen.getByLabelText("Capture"), { target: { value: "Buy soil" } });
    const energySlider = screen.getByRole("slider", { name: "Energy" });
    fireEvent.keyDown(energySlider, { key: "End" });
    fireEvent.change(screen.getByLabelText("Context"), { target: { value: "@garden" } });

    fireEvent.click(screen.getByRole("button", { name: /add to triage/i }));
    rerender(taskState({ lastCapture: { seed: "s1", kind: "ok", id: "item-9", error: null } }));

    expect(screen.getByRole("slider", { name: "Energy" }).getAttribute("aria-valuenow")).toBe("-1");
    expect((screen.getByLabelText("Context") as HTMLSelectElement).value).toBe("");
  });

  // #208 made the capture box's Energy/Size/Context genuinely persist, so
  // this caption's old real-arm suffix — "(not yet stored on a real
  // capture)" — became false, on the very arm that now DOES store them.
  // Asserted verbatim so it cannot silently rot again.
  it("does not claim the capture meta is unstored", () => {
    renderTriage(taskState());
    expect(
      screen.getByText("optional — stage, dates and everything else are decided at mint time"),
    ).toBeDefined();
    expect(screen.queryByText(/not yet stored/i)).toBeNull();
  });

  // The capture half of issue #222's rule, which that issue deliberately
  // scoped out: a write that has not been acknowledged yet must not take the
  // reader's work with it. Since #208 that work is the title PLUS three
  // selections.
  it("keeps the whole draft while the capture is still in flight", () => {
    renderTriage(taskState());
    fireEvent.change(screen.getByLabelText("Capture"), { target: { value: "Buy soil" } });
    fireEvent.keyDown(screen.getByRole("slider", { name: "Energy" }), { key: "End" });
    fireEvent.keyDown(screen.getByRole("slider", { name: "Size" }), { key: "End" });
    fireEvent.change(screen.getByLabelText("Context"), { target: { value: "@garden" } });

    fireEvent.click(screen.getByRole("button", { name: /add to triage/i }));

    // No result has come back — neither ok nor failed.
    expect((screen.getByLabelText("Capture") as HTMLInputElement).value).toBe("Buy soil");
    expect(screen.getByRole("slider", { name: "Energy" }).getAttribute("aria-valuenow")).toBe("2");
    expect(screen.getByRole("slider", { name: "Size" }).getAttribute("aria-valuenow")).toBe("2");
    expect((screen.getByLabelText("Context") as HTMLSelectElement).value).toBe("@garden");
  });

  it("keeps the title and all three meta fields when the capture comes back failed", () => {
    const { rerender } = renderTriage(taskState());
    fireEvent.change(screen.getByLabelText("Capture"), { target: { value: "Buy soil" } });
    fireEvent.keyDown(screen.getByRole("slider", { name: "Energy" }), { key: "End" });
    fireEvent.keyDown(screen.getByRole("slider", { name: "Size" }), { key: "Home" });
    fireEvent.change(screen.getByLabelText("Context"), { target: { value: "@garden" } });
    fireEvent.click(screen.getByRole("button", { name: /add to triage/i }));

    rerender(
      taskState({ lastCapture: { seed: "s1", kind: "failed", id: null, error: "Offline." } }),
    );

    expect(screen.getByText("Offline.")).toBeDefined();
    expect((screen.getByLabelText("Capture") as HTMLInputElement).value).toBe("Buy soil");
    expect(screen.getByRole("slider", { name: "Energy" }).getAttribute("aria-valuenow")).toBe("2");
    expect(screen.getByRole("slider", { name: "Size" }).getAttribute("aria-valuenow")).toBe("0");
    expect((screen.getByLabelText("Context") as HTMLSelectElement).value).toBe("@garden");
  });

  // The seed guard, from the other side: a result already processed must not
  // clear a draft the reader has started since. Without it, every unrelated
  // re-render carrying the same stale `lastCapture` would wipe the box.
  it("does not re-clear a draft typed after an already-processed ok result", () => {
    const ok = { seed: "s1", kind: "ok", id: "item-9", error: null } as const;
    const { rerender } = renderTriage(taskState({ lastCapture: ok }));

    fireEvent.change(screen.getByLabelText("Capture"), { target: { value: "Second thought" } });
    // Same seed, an unrelated re-render (a sync outcome, say).
    rerender(taskState({ lastCapture: ok, syncOutcomeSeq: 3 }));

    expect((screen.getByLabelText("Capture") as HTMLInputElement).value).toBe("Second thought");
  });

  it("announces a failed capture to a screen reader", () => {
    renderTriage(
      taskState({ lastCapture: { seed: "s1", kind: "failed", id: null, error: "Offline." } }),
    );
    expect(screen.getByRole("alert").textContent).toBe("Offline.");
  });

  it("surfaces a failed capture near the capture box", () => {
    renderTriage(
      taskState({ lastCapture: { seed: "s1", kind: "failed", id: null, error: "Offline and full." } }),
    );
    expect(screen.getByText("Offline and full.")).toBeDefined();
  });

  it("says nothing after a capture that reads as ok", () => {
    renderTriage(taskState({ lastCapture: { seed: "s1", kind: "ok", id: "item-9", error: null } }));
    expect(screen.queryByText(/didn't go through/i)).toBeNull();
  });

  it("clears a stale capture failure once a later capture succeeds", () => {
    const { rerender } = renderTriage(
      taskState({ lastCapture: { seed: "s1", kind: "failed", id: null, error: "Nope." } }),
    );
    expect(screen.getByText("Nope.")).toBeDefined();

    rerender(taskState({ lastCapture: { seed: "s2", kind: "ok", id: "item-9", error: null } }));
    expect(screen.queryByText("Nope.")).toBeNull();
  });
});

describe("TriageScreen — the inbox", () => {
  it("marks a pending capture and refuses to promote it", () => {
    renderTriage(
      taskState({ triageInbox: [itemDTO({ id: "i1", title: "Queued offline", pending: true })] }),
    );
    expect(screen.getByText("Pending")).toBeDefined();
    expect(
      screen.getByRole("button", { name: /promote to ready/i }).hasAttribute("disabled"),
    ).toBe(true);
  });

  it("sends a multi-field triage as exactly ONE call carrying every drafted field", () => {
    // #111's acceptance criterion. One call, not one per field — a 409 must
    // rebase or dead-letter the whole edit together.
    const { onTriage } = renderTriage(
      taskState({
        triageInbox: [itemDTO({ id: "i1", title: "vague thing" })],
        projects: [projectDTO({ id: "p1", name: "Kitchen rebuild" })],
      }),
    );

    fireEvent.change(rowField("Title"), { target: { value: "Order the worktop" } });
    fireEvent.change(rowField("Project"), { target: { value: "p1" } });
    fireEvent.change(rowField("Size"), { target: { value: "deep" } });
    fireEvent.change(rowField("Energy"), { target: { value: "high" } });
    fireEvent.change(rowField("Context"), { target: { value: "@computer" } });
    fireEvent.click(screen.getByRole("button", { name: /promote to ready/i }));

    expect(onTriage).toHaveBeenCalledTimes(1);
    expect(onTriage).toHaveBeenCalledWith("i1", "ready", {
      title: "Order the worktop",
      projectId: "p1",
      size: "deep",
      energy: "high",
      context: "@computer",
    });
  });

  it("leaves every untouched field alone rather than sending an empty string", () => {
    const { onTriage } = renderTriage(
      taskState({ triageInbox: [itemDTO({ id: "i1", title: "vague thing" })] }),
    );
    fireEvent.click(screen.getByRole("button", { name: /send to grilling/i }));
    expect(onTriage).toHaveBeenCalledWith("i1", "grilling", {
      title: null,
      projectId: null,
      size: null,
      energy: null,
      context: null,
    });
  });

  it("keeps the draft after promoting, until the result comes back ok", () => {
    // Issue #222: the draft used to clear the instant Promote was clicked,
    // optimistically — this is what makes a failed triage lose the reader's
    // edits on top of saying nothing about the failure.
    const { onTriage, rerender } = renderTriage(
      taskState({ triageInbox: [itemDTO({ id: "i1", title: "vague thing" })] }),
    );
    const title = rowField("Title") as HTMLInputElement;
    fireEvent.change(title, { target: { value: "Order the worktop" } });
    fireEvent.click(screen.getByRole("button", { name: /promote to ready/i }));
    expect(onTriage).toHaveBeenCalledTimes(1);
    // Still in flight — no result has come back yet.
    expect((rowField("Title") as HTMLInputElement).value).toBe("Order the worktop");

    rerender(
      taskState({
        triageInbox: [itemDTO({ id: "i1", title: "vague thing" })],
        lastTriage: { seed: "s1", itemId: "i1", kind: "ok", error: null },
      }),
    );
    expect((rowField("Title") as HTMLInputElement).value).toBe("");
  });

  it("surfaces a failed triage and leaves the draft in place", () => {
    const { onTriage, rerender } = renderTriage(
      taskState({ triageInbox: [itemDTO({ id: "i1", title: "vague thing" })] }),
    );
    const title = rowField("Title") as HTMLInputElement;
    fireEvent.change(title, { target: { value: "Order the worktop" } });
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
    expect((rowField("Title") as HTMLInputElement).value).toBe("Order the worktop");
  });

  it("announces a failed triage to a screen reader", () => {
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

  it("renders no triage form at all without an onTriage handler", () => {
    renderTriage(taskState({ triageInbox: [itemDTO({ id: "i1" })] }), { withTriage: false });
    expect(screen.queryByRole("button", { name: /promote to ready/i })).toBeNull();
  });

  it("says the inbox is empty when it is", () => {
    renderTriage(taskState());
    expect(screen.getByText("Triage is empty")).toBeDefined();
  });
});
