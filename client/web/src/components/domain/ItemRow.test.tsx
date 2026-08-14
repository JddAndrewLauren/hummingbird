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

  // #446: rows carried size as a bare muted word and no energy at all. Both
  // are now a glyph in the level's ramp colour and **no word** — the row
  // annotates a title, and two spelled-out dimensions per line competed with
  // it. The word survives on `ItemDetailPanel` only (ADR-0024).
  it("draws size and energy as glyphs, with no word on the row", () => {
    render(<ItemRow title="An action" size="normal" energy="high" />);
    expect(screen.queryByText(/QUICK|NORMAL|DEEP/)).toBeNull();
    expect(screen.queryByText(/LOW|MEDIUM|HIGH/)).toBeNull();
    // Silent to the eye is not silent to a screen reader: the chip names
    // itself, which is the whole licence for dropping the word.
    expect(screen.getByTitle("Size: normal")).toBeDefined();
    expect(screen.getByTitle("Energy: high")).toBeDefined();
  });

  it("colours each glyph by its level, and draws the level's own ramp", () => {
    render(<ItemRow title="An action" size="deep" energy="low" />);
    // The chip is found by its accessible name now that it carries no text,
    // and it is the element holding both the colour and the glyph — so
    // reading colour here reads the one the icon inherits.
    const size = screen.getByTitle("Size: deep");
    const energy = screen.getByTitle("Energy: low");
    expect(size.style.color).toBe("var(--urgency-now)");
    expect(energy.style.color).toBe("var(--status-done-fg)");
    // `not.toBeNull`, not `toBeDefined`: a missing glyph returns `null`,
    // which *is* defined, so the weaker assertion passes on no icon at all.
    const rings = size.querySelector("svg");
    expect(rings).not.toBeNull();
    // And the ramp itself, since it is opacity rather than colour and so
    // survives every style assertion above: `deep` earns all three rings.
    expect(Array.from(rings!.children, (el) => el.getAttribute("opacity"))).toEqual(["1", "1", "1"]);
  });

  // The row's documented contract for every optional chip: nothing to say,
  // nothing rendered. The unset ghost glyph belongs on `ItemDetailPanel`,
  // the one surface that describes a single item in full — a dense list is
  // not the place to draw an unmade judgement on every line.
  it("omits both entirely when the caller has nothing to say", () => {
    render(<ItemRow title="An unjudged action" />);
    expect(screen.queryByText("—")).toBeNull();
    expect(screen.queryByText(/QUICK|NORMAL|DEEP/)).toBeNull();
    expect(screen.queryByText(/LOW|MEDIUM|HIGH/)).toBeNull();
    // Now that the chip is word-free, its title is the only thing left to
    // leak an unjudged dimension — and a ghost glyph here would be
    // indistinguishable from `deep` without a word beside it.
    expect(screen.queryByTitle(/^Size:/)).toBeNull();
    expect(screen.queryByTitle(/^Energy:/)).toBeNull();
  });
});
