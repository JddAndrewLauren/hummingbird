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
  render(
    <TriageScreen
      demo={null}
      task={task}
      onSubmitCapture={onSubmitCapture}
      onTriage={options.withTriage === false ? undefined : onTriage}
      focusRequestId={0}
    />,
  );
  return { onSubmitCapture, onTriage };
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

  it("submits the draft and clears the box", () => {
    const { onSubmitCapture } = renderTriage(taskState());
    const input = screen.getByLabelText("Capture") as HTMLInputElement;

    fireEvent.change(input, { target: { value: "Call the plumber" } });
    fireEvent.click(screen.getByRole("button", { name: /add to triage/i }));

    expect(onSubmitCapture).toHaveBeenCalledTimes(1);
    expect(onSubmitCapture.mock.calls[0][0]).toBe("Call the plumber");
    expect(input.value).toBe("");
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

  it("clears the Energy/Size/Context controls back to rest after submit", () => {
    renderTriage(taskState());
    fireEvent.change(screen.getByLabelText("Capture"), { target: { value: "Buy soil" } });
    const energySlider = screen.getByRole("slider", { name: "Energy" });
    fireEvent.keyDown(energySlider, { key: "End" });
    fireEvent.change(screen.getByLabelText("Context"), { target: { value: "@garden" } });

    fireEvent.click(screen.getByRole("button", { name: /add to triage/i }));

    expect(energySlider.getAttribute("aria-valuenow")).toBe("-1");
    expect((screen.getByLabelText("Context") as HTMLSelectElement).value).toBe("");
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

  it("clears the draft once promoted, so a re-rendered row starts blank", () => {
    const { onTriage } = renderTriage(
      taskState({ triageInbox: [itemDTO({ id: "i1", title: "vague thing" })] }),
    );
    const title = rowField("Title") as HTMLInputElement;
    fireEvent.change(title, { target: { value: "Order the worktop" } });
    fireEvent.click(screen.getByRole("button", { name: /promote to ready/i }));
    expect(onTriage).toHaveBeenCalledTimes(1);
    expect((rowField("Title") as HTMLInputElement).value).toBe("");
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
