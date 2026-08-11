// @vitest-environment jsdom

// The regression suite for the shell's capture popover and the box inside it.
// It inherited what `TriageScreen.test.tsx` used to hold for the capture box
// (#110 / PR #206) and adds the two things this move introduced: the second
// destination, and the overlay's own keyboard contract.
//
// The deciding logic is unit-tested elsewhere — `canSubmitCapture` refuses an
// empty draft, `capture-destination.ts` names the two stages. What no node
// test can reach is whether the buttons actually consult them, whether Enter
// still means Triage and not the mint, and whether Escape reaches `onClose`
// from inside the card. That thread is what these mount.

import { describe, expect, it, vi } from "vitest";
import { CapturePopover } from "./CapturePopover";
import { fireEvent, render, screen } from "../test/component";

function renderPopover(options: { open?: boolean } = {}) {
  const onSubmit = vi.fn();
  const onClose = vi.fn();
  render(
    <CapturePopover
      open={options.open ?? true}
      focusRequestId={1}
      onClose={onClose}
      onSubmit={onSubmit}
      demo={false}
    />,
  );
  return { onSubmit, onClose };
}

function field(): HTMLInputElement {
  return screen.getByLabelText("Capture") as HTMLInputElement;
}

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
    const add = screen.getByRole("button", { name: /add to triage/i });
    const mint = screen.getByRole("button", { name: /mint action/i });

    expect(add.hasAttribute("disabled")).toBe(true);
    expect(mint.hasAttribute("disabled")).toBe(true);

    fireEvent.change(field(), { target: { value: "   " } });
    expect(add.hasAttribute("disabled")).toBe(true);
    expect(mint.hasAttribute("disabled")).toBe(true);

    fireEvent.change(field(), { target: { value: "Call the plumber" } });
    expect(add.hasAttribute("disabled")).toBe(false);
    expect(mint.hasAttribute("disabled")).toBe(false);
  });

  it("sends the raw draft to Triage and clears the box", () => {
    const { onSubmit } = renderPopover();
    fireEvent.change(field(), { target: { value: "  Buy   OAT milk  " } });
    fireEvent.click(screen.getByRole("button", { name: /add to triage/i }));

    expect(onSubmit).toHaveBeenCalledTimes(1);
    expect(onSubmit).toHaveBeenCalledWith("  Buy   OAT milk  ", "triage");
    expect(field().value).toBe("");
  });

  it("sends the skip — Mint action captures straight into Ready", () => {
    const { onSubmit } = renderPopover();
    fireEvent.change(field(), { target: { value: "Order the worktop" } });
    fireEvent.click(screen.getByRole("button", { name: /mint action/i }));

    expect(onSubmit).toHaveBeenCalledTimes(1);
    expect(onSubmit).toHaveBeenCalledWith("Order the worktop", "ready");
  });

  it("submits to Triage on Enter, never to Ready", () => {
    const { onSubmit } = renderPopover();
    fireEvent.change(field(), { target: { value: "Call the plumber" } });
    fireEvent.keyDown(field(), { key: "Enter" });
    expect(onSubmit).toHaveBeenCalledWith("Call the plumber", "triage");
  });

  it("does not submit on the Enter that commits an IME composition", () => {
    const { onSubmit } = renderPopover();
    fireEvent.change(field(), { target: { value: "植物に水をやる" } });
    fireEvent.keyDown(field(), { key: "Enter", isComposing: true });
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("stays open and says where the capture went, since nothing else on screen can", () => {
    const { onSubmit, onClose } = renderPopover();
    fireEvent.change(field(), { target: { value: "Call the plumber" } });
    fireEvent.click(screen.getByRole("button", { name: /add to triage/i }));
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByText(/Added to Triage — Call the plumber/)).toBeDefined();
    expect(document.activeElement).toBe(field());

    fireEvent.change(field(), { target: { value: "Order the worktop" } });
    fireEvent.click(screen.getByRole("button", { name: /mint action/i }));
    expect(screen.getByText(/Minted into Ready — Order the worktop/)).toBeDefined();
    expect(onSubmit).toHaveBeenCalledTimes(2);
  });
});
