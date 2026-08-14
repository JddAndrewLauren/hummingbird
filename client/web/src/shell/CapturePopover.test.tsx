// @vitest-environment jsdom

// The regression suite for the shell's capture popover and the box inside it.
// It inherited what `TriageScreen.test.tsx` used to hold for the capture box
// (#110 / PR #206, then #208's Energy/Size/Context and #222's clear-on-ok
// rule) and adds the two things the move into the shell introduced: the
// second destination, and the overlay's own keyboard contract.
//
// The deciding logic is unit-tested elsewhere — `canSubmitCapture` refuses an
// empty draft, `capture-destination.ts` names the two stages,
// `capture-meta.ts` resolves the sliders onto the wire vocabulary. What no
// node test can reach is whether the buttons actually consult them, whether
// Enter still means Triage and not the mint, and whether Escape reaches
// `onClose` from inside the card. That thread is what these mount.

import { describe, expect, it, vi } from "vitest";
import { CapturePopover } from "./CapturePopover";
import type { ProjectDTO } from "../store/protocol";
import type { TaskCaptureResult } from "../store/store";
import { fireEvent, render, screen } from "../test/component";

function renderPopover(
  options: {
    open?: boolean;
    demo?: boolean;
    lastCapture?: TaskCaptureResult | null;
    projects?: ProjectDTO[];
  } = {},
) {
  const onSubmit = vi.fn();
  const onClose = vi.fn();
  const props = (lastCapture: TaskCaptureResult | null) => ({
    open: options.open ?? true,
    focusRequestId: 1,
    onClose,
    onSubmit,
    projects: options.projects ?? [],
    demo: options.demo ?? false,
    lastCapture,
  });
  const view = render(<CapturePopover {...props(options.lastCapture ?? null)} />);
  const rerender = (lastCapture: TaskCaptureResult | null) =>
    view.rerender(<CapturePopover {...props(lastCapture)} />);
  return { onSubmit, onClose, rerender };
}

function field(): HTMLInputElement {
  return screen.getByLabelText("Capture") as HTMLInputElement;
}

/** Every optional field left at rest — what `resolveCaptureFields` hands
 * `onSubmit` when nothing beside the title was touched. */
const NO_FIELDS = {
  size: null,
  energy: null,
  context: null,
  description: null,
  projectId: null,
  priority: null,
  deadline: null,
  scheduledDate: null,
};

describe("CapturePopover — the overlay", () => {
  it("renders nothing at all while closed", () => {
    renderPopover({ open: false });
    expect(screen.queryByLabelText("Capture")).toBeNull();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("opens as a dialog with the capture field already focused", () => {
    renderPopover();
    expect(screen.getByRole("dialog").getAttribute("aria-modal")).toBe("true");
    expect(document.activeElement).toBe(field());
  });

  it("closes on Escape from inside the card, on the close button, and on the scrim", () => {
    // Escape is bound to the document, not the markup, so it must still work
    // with focus in the field — which is where it always is on open.
    const first = renderPopover();
    fireEvent.keyDown(field(), { key: "Escape" });
    expect(first.onClose).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(first.onClose).toHaveBeenCalledTimes(2);

    // The scrim is the dialog's parent element; a press on the card itself
    // must not close (a drag that ends outside is not a request to close).
    const dialog = screen.getByRole("dialog");
    fireEvent.mouseDown(dialog);
    expect(first.onClose).toHaveBeenCalledTimes(2);
    fireEvent.mouseDown(dialog.parentElement as HTMLElement);
    expect(first.onClose).toHaveBeenCalledTimes(3);
  });
});

describe("CapturePopover — the capture box", () => {
  it("refuses an empty or whitespace-only draft on both destinations", () => {
    renderPopover();
    const add = screen.getByRole("button", { name: "Triage" });
    const mint = screen.getByRole("button", { name: "Mint action" });

    expect(add.hasAttribute("disabled")).toBe(true);
    expect(mint.hasAttribute("disabled")).toBe(true);

    fireEvent.change(field(), { target: { value: "   " } });
    expect(add.hasAttribute("disabled")).toBe(true);
    expect(mint.hasAttribute("disabled")).toBe(true);

    fireEvent.change(field(), { target: { value: "Call the plumber" } });
    expect(add.hasAttribute("disabled")).toBe(false);
    expect(mint.hasAttribute("disabled")).toBe(false);
  });

  it("sends the raw draft to Triage", () => {
    const { onSubmit } = renderPopover();
    fireEvent.change(field(), { target: { value: "  Buy   OAT milk  " } });
    fireEvent.click(screen.getByRole("button", { name: "Triage" }));

    expect(onSubmit).toHaveBeenCalledTimes(1);
    expect(onSubmit).toHaveBeenCalledWith("  Buy   OAT milk  ", "triage", NO_FIELDS);
  });

  it("sends the skip — the mint button captures straight into Ready", () => {
    const { onSubmit } = renderPopover();
    fireEvent.change(field(), { target: { value: "Order the worktop" } });
    fireEvent.click(screen.getByRole("button", { name: "Mint action" }));

    expect(onSubmit).toHaveBeenCalledTimes(1);
    expect(onSubmit).toHaveBeenCalledWith("Order the worktop", "ready", NO_FIELDS);
  });

  it("submits to Triage on Enter, never to Ready", () => {
    const { onSubmit } = renderPopover();
    fireEvent.change(field(), { target: { value: "Call the plumber" } });
    fireEvent.keyDown(field(), { key: "Enter" });
    expect(onSubmit).toHaveBeenCalledWith("Call the plumber", "triage", NO_FIELDS);
  });

  it("does not submit on the Enter that commits an IME composition", () => {
    const { onSubmit } = renderPopover();
    fireEvent.change(field(), { target: { value: "植物に水をやる" } });
    fireEvent.keyDown(field(), { key: "Enter", isComposing: true });
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("stays open and says where the capture went once the result comes back ok", () => {
    const { onSubmit, onClose, rerender } = renderPopover();
    fireEvent.change(field(), { target: { value: "Call the plumber" } });
    fireEvent.click(screen.getByRole("button", { name: "Triage" }));
    expect(onClose).not.toHaveBeenCalled();

    rerender({ seed: "s1", kind: "ok", id: "item-9", error: null });
    expect(screen.getByText(/Added to Triage — Call the plumber/)).toBeDefined();
    expect(field().value).toBe("");
    expect(document.activeElement).toBe(field());

    fireEvent.change(field(), { target: { value: "Order the worktop" } });
    fireEvent.click(screen.getByRole("button", { name: "Mint action" }));
    rerender({ seed: "s2", kind: "ok", id: "item-10", error: null });
    expect(screen.getByText(/Minted into Ready — Order the worktop/)).toBeDefined();
    expect(onSubmit).toHaveBeenCalledTimes(2);
  });
});

// #208's headline acceptance, proved from the rendered controls themselves —
// not just that `resolveCaptureFields` (the pure layer) accepts a
// `CaptureMeta`. The Energy/Size sliders are `role="slider"` elements moved
// with the keyboard (`End` jumps to the last stop, per `Slider.tsx`'s own
// `onKeyDown`), never a plain `<input>`.
describe("CapturePopover — the full field set behind More details", () => {
  const openDetails = () =>
    fireEvent.click(screen.getByRole("button", { name: /more details/i }));

  it("keeps every mint-time field shut until asked", () => {
    // One line and Enter is the fastest thing on the screen, and a form that
    // opens to seven fields taxes every capture for a decision most of them
    // do not make.
    renderPopover();

    expect(screen.queryByLabelText("Description")).toBeNull();
    expect(screen.queryByLabelText("Project")).toBeNull();
    expect(screen.queryByLabelText("Priority")).toBeNull();
    expect(screen.queryByLabelText("Deadline")).toBeNull();
    expect(screen.queryByLabelText("Scheduled date")).toBeNull();
    expect(
      screen.getByRole("button", { name: /more details/i }).getAttribute("aria-expanded"),
    ).toBe("false");
  });

  it("carries every revealed field onto the submit, in one capture", () => {
    const { onSubmit } = renderPopover({
      projects: [
        {
          id: "proj-1",
          name: "Kitchen",
          description: null,
          archivedAt: null,
          createdAt: 0,
          updatedAt: 0,
          version: 0,
        } as ProjectDTO,
      ],
    });
    fireEvent.change(field(), { target: { value: "Order the worktop" } });
    openDetails();

    fireEvent.change(screen.getByLabelText("Description"), {
      target: { value: "the oak one" },
    });
    fireEvent.change(screen.getByLabelText("Project"), { target: { value: "proj-1" } });
    fireEvent.change(screen.getByLabelText("Priority"), { target: { value: "2" } });
    fireEvent.change(screen.getByLabelText("Deadline"), { target: { value: "2026-09-01" } });
    fireEvent.change(screen.getByLabelText("Scheduled date"), {
      target: { value: "2026-08-30" },
    });

    fireEvent.click(screen.getByRole("button", { name: "Triage" }));

    expect(onSubmit).toHaveBeenCalledTimes(1);
    expect(onSubmit).toHaveBeenCalledWith("Order the worktop", "triage", {
      ...NO_FIELDS,
      description: "the oak one",
      projectId: "proj-1",
      priority: 2,
      deadline: "2026-09-01",
      scheduledDate: "2026-08-30",
    });
  });

  it("names an hour through the deadline field's own second gesture", () => {
    const { onSubmit } = renderPopover();
    fireEvent.change(field(), { target: { value: "Call the vet" } });
    openDetails();
    fireEvent.change(screen.getByLabelText("Deadline"), { target: { value: "2026-09-01" } });
    fireEvent.click(screen.getByRole("button", { name: "Add time" }));
    fireEvent.change(screen.getByLabelText("Time"), { target: { value: "09:30" } });

    fireEvent.click(screen.getByRole("button", { name: "Triage" }));

    expect(onSubmit).toHaveBeenCalledWith("Call the vet", "triage", {
      ...NO_FIELDS,
      deadline: "2026-09-01T09:30",
    });
  });

  it("cannot be given an impossible date at all — the controls are pickers", () => {
    // Both date fields are `input[type=date]`, which refuses to hold
    // `2026-02-30`; the value never reaches the form's state, so there is
    // nothing for the submit gate to catch. That gate still runs
    // (`captureMetaProblems`, unit-tested against the same rules triage
    // uses) — this is the pin on WHY it never fires here, so a later change
    // back to a free-text field is a visibly different test rather than a
    // silently unguarded form.
    renderPopover();
    fireEvent.change(field(), { target: { value: "Call the vet" } });
    openDetails();
    fireEvent.change(screen.getByLabelText("Scheduled date"), {
      target: { value: "2026-02-30" },
    });

    expect((screen.getByLabelText("Scheduled date") as HTMLInputElement).value).toBe("");
    expect(screen.getByRole("button", { name: "Triage" }).hasAttribute("disabled")).toBe(false);
    expect(screen.queryByText("Use YYYY-MM-DD")).toBeNull();
  });

  it("keeps the revealed fields while a capture is in flight, and shuts them on ok", () => {
    // #222's rule, now covering five more fields than it did: a failed write
    // must not take the reader's typing with it.
    const { rerender } = renderPopover();
    fireEvent.change(field(), { target: { value: "Call the vet" } });
    openDetails();
    fireEvent.change(screen.getByLabelText("Description"), { target: { value: "about Bess" } });

    rerender({ kind: "failed", seed: "seed-1", id: null, error: "boom" });
    expect((screen.getByLabelText("Description") as HTMLTextAreaElement).value).toBe("about Bess");

    rerender({ kind: "ok", seed: "seed-2", id: "item-1", error: null });
    // Shut again, and empty behind the disclosure — the next capture starts
    // clean rather than inheriting this one's decisions.
    expect(screen.queryByLabelText("Description")).toBeNull();
    openDetails();
    expect((screen.getByLabelText("Description") as HTMLTextAreaElement).value).toBe("");
  });
});

describe("CapturePopover — the capture meta (#208)", () => {
  it("carries the Energy/Size/Context selections onto the submit", () => {
    const { onSubmit } = renderPopover();
    fireEvent.change(field(), { target: { value: "Buy soil" } });
    fireEvent.keyDown(screen.getByRole("slider", { name: "Energy" }), { key: "End" });
    fireEvent.keyDown(screen.getByRole("slider", { name: "Size" }), { key: "End" });
    fireEvent.change(screen.getByLabelText("Context"), { target: { value: "@garden" } });

    fireEvent.click(screen.getByRole("button", { name: "Triage" }));

    expect(onSubmit).toHaveBeenCalledWith("Buy soil", "triage", {
      ...NO_FIELDS,
      size: "deep",
      energy: "high",
      context: "@garden",
    });
  });

  it("sends only the one field the reader set, leaving the other two absent", () => {
    const { onSubmit } = renderPopover();
    fireEvent.change(field(), { target: { value: "Buy soil" } });
    fireEvent.keyDown(screen.getByRole("slider", { name: "Energy" }), { key: "End" });

    fireEvent.click(screen.getByRole("button", { name: "Triage" }));

    expect(onSubmit).toHaveBeenCalledWith("Buy soil", "triage", {
      ...NO_FIELDS,
      energy: "high",
    });
  });

  it("clears the Energy/Size/Context controls back to rest on an ok result", () => {
    const { rerender } = renderPopover();
    fireEvent.change(field(), { target: { value: "Buy soil" } });
    fireEvent.keyDown(screen.getByRole("slider", { name: "Energy" }), { key: "End" });
    fireEvent.change(screen.getByLabelText("Context"), { target: { value: "@garden" } });

    fireEvent.click(screen.getByRole("button", { name: "Triage" }));
    rerender({ seed: "s1", kind: "ok", id: "item-9", error: null });

    expect(screen.getByRole("slider", { name: "Energy" }).getAttribute("aria-valuenow")).toBe("-1");
    expect((screen.getByLabelText("Context") as HTMLSelectElement).value).toBe("");
  });

  // The caption that used to sit under these controls said stage and dates
  // were "decided at mint time". They can be decided here now — the "More
  // details" disclosure is where — so the sentence was deleted rather than
  // reworded, and this is the pin against it coming back.
  it("makes no claim about what capture cannot set", () => {
    renderPopover();
    expect(screen.queryByText(/decided at mint time/i)).toBeNull();
    expect(screen.queryByText(/not yet stored/i)).toBeNull();
  });
});

// The capture half of issue #222's rule, which that issue deliberately scoped
// out: a write that has not been acknowledged yet must not take the reader's
// work with it. Since #208 that work is the title PLUS three selections.
describe("CapturePopover — the clear-on-ok rule (#222)", () => {
  it("keeps the whole draft while the capture is still in flight", () => {
    renderPopover();
    fireEvent.change(field(), { target: { value: "Buy soil" } });
    fireEvent.keyDown(screen.getByRole("slider", { name: "Energy" }), { key: "End" });
    fireEvent.keyDown(screen.getByRole("slider", { name: "Size" }), { key: "End" });
    fireEvent.change(screen.getByLabelText("Context"), { target: { value: "@garden" } });

    fireEvent.click(screen.getByRole("button", { name: "Triage" }));

    // No result has come back — neither ok nor failed.
    expect(field().value).toBe("Buy soil");
    expect(screen.getByRole("slider", { name: "Energy" }).getAttribute("aria-valuenow")).toBe("2");
    expect(screen.getByRole("slider", { name: "Size" }).getAttribute("aria-valuenow")).toBe("2");
    expect((screen.getByLabelText("Context") as HTMLSelectElement).value).toBe("@garden");
  });

  it("keeps the title and all three meta fields when the capture comes back failed", () => {
    const { rerender } = renderPopover();
    fireEvent.change(field(), { target: { value: "Buy soil" } });
    fireEvent.keyDown(screen.getByRole("slider", { name: "Energy" }), { key: "End" });
    fireEvent.keyDown(screen.getByRole("slider", { name: "Size" }), { key: "Home" });
    fireEvent.change(screen.getByLabelText("Context"), { target: { value: "@garden" } });
    fireEvent.click(screen.getByRole("button", { name: "Triage" }));

    rerender({ seed: "s1", kind: "failed", id: null, error: "Offline." });

    expect(screen.getByText("Offline.")).toBeDefined();
    expect(field().value).toBe("Buy soil");
    expect(screen.getByRole("slider", { name: "Energy" }).getAttribute("aria-valuenow")).toBe("2");
    expect(screen.getByRole("slider", { name: "Size" }).getAttribute("aria-valuenow")).toBe("0");
    expect((screen.getByLabelText("Context") as HTMLSelectElement).value).toBe("@garden");
  });

  // The seed guard, from the other side: a result already processed must not
  // clear a draft the reader has started since. Without it, every unrelated
  // re-render carrying the same stale `lastCapture` would wipe the box.
  it("does not re-clear a draft typed after an already-processed ok result", () => {
    const ok: TaskCaptureResult = { seed: "s1", kind: "ok", id: "item-9", error: null };
    const { rerender } = renderPopover({ lastCapture: ok });

    fireEvent.change(field(), { target: { value: "Second thought" } });
    // Same seed, an unrelated re-render (a sync outcome, say).
    rerender(ok);

    expect(field().value).toBe("Second thought");
  });

  it("announces a failed capture to a screen reader", () => {
    renderPopover({ lastCapture: { seed: "s1", kind: "failed", id: null, error: "Offline." } });
    expect(screen.getByRole("alert").textContent).toBe("Offline.");
  });

  it("says nothing after a capture that reads as ok", () => {
    renderPopover({ lastCapture: { seed: "s1", kind: "ok", id: "item-9", error: null } });
    expect(screen.queryByText(/didn't go through/i)).toBeNull();
  });

  it("clears a stale capture failure once a later capture succeeds", () => {
    const { rerender } = renderPopover({
      lastCapture: { seed: "s1", kind: "failed", id: null, error: "Nope." },
    });
    expect(screen.getByText("Nope.")).toBeDefined();

    rerender({ seed: "s2", kind: "ok", id: "item-9", error: null });
    expect(screen.queryByText("Nope.")).toBeNull();
  });

  it("clears and reports right away in demo mode, where no result is coming", () => {
    const { onSubmit } = renderPopover({ demo: true });
    fireEvent.change(field(), { target: { value: "Call the plumber" } });
    fireEvent.click(screen.getByRole("button", { name: "Triage" }));

    expect(onSubmit).toHaveBeenCalledTimes(1);
    expect(field().value).toBe("");
    expect(screen.getByText(/Added to Triage — Call the plumber/)).toBeDefined();
  });

  it("never wears a stale failure in demo mode", () => {
    renderPopover({
      demo: true,
      lastCapture: { seed: "s1", kind: "failed", id: null, error: "Nope." },
    });
    expect(screen.queryByText("Nope.")).toBeNull();
  });
});
