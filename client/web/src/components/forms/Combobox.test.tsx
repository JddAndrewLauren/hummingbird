// @vitest-environment jsdom

// The gate this control did not have. It was a native `<datalist>` until
// #641 made capture's Context sticky across submits, at which point the
// browser's own substring filter — applied against the value sitting in the
// box — hid every option but one, and the decorative chevron gave no way out
// of it. A newly-minted `@shopping` was in the vocabulary, in the props, and
// unreachable on screen.
//
// So the assertions below are mostly about *reachability*: with a value in
// the field, does asking for the list produce the whole list. The filtering
// rule itself is `combobox-options.test.ts`'s, exhaustively; what no node
// test can reach is whether the chevron is wired to it at all.

import { describe, expect, it, vi } from "vitest";
import { useState } from "react";
import { Combobox } from "./Combobox";
import { comboboxOpenSignal } from "./combobox-open";
import { act, fireEvent, render, screen } from "../../test/component";

const CONTEXTS = ["@home", "@computer", "@phone", "@errands", "@garden", "@shopping"];

/** The control as a caller actually holds it — controlled, with the parent
 * owning the value. An uncontrolled render cannot show a commit landing. */
function renderCombobox(initial = "") {
  const onChange = vi.fn();
  function Host() {
    const [value, setValue] = useState(initial);
    return (
      <Combobox
        label="Context"
        value={value}
        suggestions={CONTEXTS}
        onChange={(next) => {
          onChange(next);
          setValue(next);
        }}
      />
    );
  }
  render(<Host />);
  return { onChange };
}

function field(): HTMLInputElement {
  return screen.getByLabelText("Context") as HTMLInputElement;
}

function chevron(): HTMLElement {
  return screen.getByRole("button", { name: "Show context suggestions" });
}

function optionText(): string[] {
  return screen.queryAllByRole("option").map((option) => option.textContent ?? "");
}

describe("Combobox", () => {
  // Symptom 2, pinned at its cause rather than at its appearance: jsdom draws
  // no native caret, but the `list` attribute that made Chromium draw one is
  // exactly what this asserts is gone. One glyph in the markup, one in the
  // browser.
  it("renders one chevron and no native datalist", () => {
    renderCombobox();
    expect(field().getAttribute("list")).toBeNull();
    expect(document.querySelector("datalist")).toBeNull();
    expect(screen.getAllByRole("button")).toHaveLength(1);

    fireEvent.focus(field());
    expect(document.querySelector("datalist")).toBeNull();
    expect(screen.getAllByRole("button")).toHaveLength(1);
  });

  it("shows nothing until it is asked", () => {
    renderCombobox("@errands");
    expect(optionText()).toEqual([]);
    expect(field().getAttribute("aria-expanded")).toBe("false");
  });

  // The regression that matters. `@errands` is in the box — as it is after
  // every capture since #641 — and the whole vocabulary must still be one
  // click away, `@shopping` included.
  it("opens the whole vocabulary from the chevron with a value already in the box", () => {
    renderCombobox("@errands");
    fireEvent.click(chevron());
    expect(optionText()).toEqual(CONTEXTS);
    expect(field().getAttribute("aria-expanded")).toBe("true");
  });

  it("escapes a filter the reader typed, without a close-then-open", () => {
    renderCombobox();
    fireEvent.change(field(), { target: { value: "ph" } });
    expect(optionText()).toEqual(["@phone"]);

    fireEvent.click(chevron());
    expect(optionText()).toEqual(CONTEXTS);
  });

  it("closes again on a second click of the chevron", () => {
    renderCombobox("@errands");
    fireEvent.click(chevron());
    fireEvent.click(chevron());
    expect(optionText()).toEqual([]);
  });

  it("filters as the reader types, and commits a clicked option", () => {
    const { onChange } = renderCombobox();
    fireEvent.change(field(), { target: { value: "ph" } });
    expect(optionText()).toEqual(["@phone"]);

    fireEvent.click(screen.getByRole("option", { name: "@phone" }));
    expect(onChange).toHaveBeenLastCalledWith("@phone");
    expect(field().value).toBe("@phone");
    expect(optionText()).toEqual([]);
  });

  it("browses from the keyboard: ArrowDown opens, Enter commits", () => {
    const { onChange } = renderCombobox("@errands");
    fireEvent.keyDown(field(), { key: "ArrowDown" });
    // Opened by an arrow, so it browses too — not the one option the sticky
    // value happens to match.
    expect(optionText()).toEqual(CONTEXTS);

    fireEvent.keyDown(field(), { key: "ArrowDown" });
    expect(field().getAttribute("aria-activedescendant")).toBe(
      screen.getByRole("option", { name: "@computer" }).id,
    );

    fireEvent.keyDown(field(), { key: "Enter" });
    expect(onChange).toHaveBeenLastCalledWith("@computer");
    expect(optionText()).toEqual([]);
  });

  // The capture box reads Enter as "capture to Triage". With no option
  // active, that Enter is still the reader's.
  it("lets Enter through when no option is active", () => {
    const onKeyDown = vi.fn();
    render(
      // A stand-in for the capture form's own key handler — the enclosing
      // handler this test is about. Not a control, and not shipped UI.
      // eslint-disable-next-line jsx-a11y/no-static-element-interactions
      <div onKeyDown={onKeyDown}>
        <Combobox label="Context" value="" suggestions={CONTEXTS} onChange={() => {}} />
      </div>,
    );
    fireEvent.click(chevron());
    fireEvent.keyDown(field(), { key: "Enter" });
    expect(onKeyDown).toHaveBeenCalled();
    expect(onKeyDown.mock.calls[0][0].isPropagationStopped()).toBe(false);
  });

  it("keeps a committing Enter away from an enclosing submit", () => {
    const onKeyDown = vi.fn();
    render(
      // A stand-in for the capture form's own key handler — the enclosing
      // handler this test is about. Not a control, and not shipped UI.
      // eslint-disable-next-line jsx-a11y/no-static-element-interactions
      <div onKeyDown={onKeyDown}>
        <Combobox label="Context" value="" suggestions={CONTEXTS} onChange={() => {}} />
      </div>,
    );
    fireEvent.keyDown(field(), { key: "ArrowDown" });
    fireEvent.keyDown(field(), { key: "Enter" });
    // The ArrowDown that opened the list still bubbles — it is not a key any
    // enclosing handler acts on. The Enter that committed does not.
    expect(onKeyDown.mock.calls.map((call) => call[0].key)).toEqual(["ArrowDown"]);
  });

  it("closes on a pointer outside it", () => {
    renderCombobox();
    fireEvent.click(chevron());
    expect(optionText()).toEqual(CONTEXTS);

    fireEvent.pointerDown(document.body);
    expect(optionText()).toEqual([]);
  });

  // Escape is not this component's to bind — `shell/escape-claimants.ts` owns
  // it for every overlay at once, and reads the flag this publishes.
  it("publishes its open state for the shell's Escape, and closes from it", () => {
    renderCombobox();
    expect(comboboxOpenSignal.getSnapshot()).toBe(false);

    fireEvent.click(chevron());
    expect(comboboxOpenSignal.getSnapshot()).toBe(true);

    act(() => comboboxOpenSignal.closeAll());
    expect(optionText()).toEqual([]);
    expect(comboboxOpenSignal.getSnapshot()).toBe(false);
  });

  it("reports itself shut once nothing matches what was typed", () => {
    renderCombobox();
    fireEvent.change(field(), { target: { value: "@boat" } });
    expect(optionText()).toEqual([]);
    expect(comboboxOpenSignal.getSnapshot()).toBe(false);
  });
});
