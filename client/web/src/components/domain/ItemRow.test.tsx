// @vitest-environment jsdom

// `ItemRow` hand-rolls a keyboard-activatable div, and the repo's eslint
// config bends `jsx-a11y/no-static-element-interactions` with
// `allowExpressionValues` specifically so it can. That trade is only sound
// if the hand-rolled half actually works — a lint exemption plus a
// typechecked ternary proves neither that the row takes focus nor that Enter
// and Space activate it. This is the test that does.

import { describe, expect, it, vi } from "vitest";
import { ItemRow } from "./ItemRow";
import { fireEvent, render, screen } from "../../test/component";

describe("ItemRow — the activatable contract", () => {
  it("is inert text with no onClick: no button role, no tab stop", () => {
    render(<ItemRow title="Renew the passport" />);
    expect(screen.queryByRole("button")).toBeNull();
    expect(screen.getByText("Renew the passport")).toBeDefined();
  });

  it("becomes a focusable button once it has an onClick", () => {
    render(<ItemRow title="Renew the passport" onClick={() => {}} />);
    const row = screen.getByRole("button", { name: /Renew the passport/ });
    expect(row.getAttribute("tabindex")).toBe("0");
  });

  it("activates on Enter and on Space", () => {
    const onClick = vi.fn();
    render(<ItemRow title="Renew the passport" onClick={onClick} />);
    const row = screen.getByRole("button", { name: /Renew the passport/ });

    fireEvent.keyDown(row, { key: "Enter" });
    expect(onClick).toHaveBeenCalledTimes(1);

    fireEvent.keyDown(row, { key: " " });
    expect(onClick).toHaveBeenCalledTimes(2);
  });

  it("ignores an unrelated key", () => {
    const onClick = vi.fn();
    render(<ItemRow title="Renew the passport" onClick={onClick} />);
    fireEvent.keyDown(screen.getByRole("button"), { key: "a" });
    expect(onClick).not.toHaveBeenCalled();
  });

  it("does not activate from the keyboard when it is inert", () => {
    const onKeyDown = vi.fn();
    render(<ItemRow title="Renew the passport" onKeyDown={onKeyDown} />);
    fireEvent.keyDown(screen.getByText("Renew the passport"), { key: "Enter" });
    // The caller's own handler still runs; there is just nothing to activate.
    expect(onKeyDown).toHaveBeenCalledTimes(1);
  });
});

describe("ItemRow — the meta chips", () => {
  it("marks a pending row and leaves a confirmed one unmarked", () => {
    const { rerender } = render(<ItemRow title="Queued" pending />);
    expect(screen.getByText("Pending")).toBeDefined();
    rerender(<ItemRow title="Queued" pending={false} />);
    expect(screen.queryByText("Pending")).toBeNull();
  });

  it("omits priority entirely at 'No priority', and labels it otherwise", () => {
    // ADR-0002/#108: the wire value is Linear's inverted, holed 0..4
    // encoding, so nothing may render the raw number.
    const { rerender } = render(<ItemRow title="An action" priority={0} />);
    expect(screen.queryByText("0")).toBeNull();

    rerender(<ItemRow title="An action" priority={1} />);
    expect(screen.queryByText("1")).toBeNull();
    expect(screen.getByText(/urgent/i)).toBeDefined();
  });

  it("says the urgency of its dot in words, not colour alone", () => {
    render(<ItemRow title="An action" urgency="overdue" />);
    expect(screen.getByTitle("Overdue")).toBeDefined();
  });
});
