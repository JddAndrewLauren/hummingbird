// @vitest-environment jsdom

// The wiring gate for ADR-0017/#311's Status screen: three of the four PRs
// in the S10-S13 batch shipped UI state with no reader, and this is exactly
// the check that would have caught it here — a mounted screen, not an
// inspected module, proving the registry's `surface: "status"` filter
// actually reaches the DOM through `StatusScreen` -> `RankedRegion` ->
// `rankPanes`.

import { describe, expect, it } from "vitest";
import { StatusScreen } from "./StatusScreen";
import { QUESTION_ORDER } from "./questions/contract";
import { QUESTIONS } from "./questions/registry";
import { render, screen, taskState } from "../test/component";

const NOW_MS = 1_700_000_000_000;

// Derived from the registry, not hardcoded: #313-#316 each replace one of
// these placeholders in turn, and a literal label array or count here would
// be a shared line every one of those four PRs has to edit. Asserting
// non-empty keeps this from degenerating into a tautology that would still
// pass against an empty registry.
const STATUS_LABELS = QUESTION_ORDER.filter((q) => QUESTIONS[q].surface === "status").map(
  (q) => QUESTIONS[q].label,
);

describe("StatusScreen", () => {
  it("renders one pane for every registered status question, and never a 'now' question", () => {
    expect(STATUS_LABELS.length).toBeGreaterThan(0);

    render(
      <StatusScreen
        demo={null}
        onScreen={() => {}}
        task={taskState()}
        nowMs={NOW_MS}
        calendarReads={{}}
        calendarConnected={false}
      />,
    );

    // Every registered status question is discoverable, even though nothing
    // has polled it yet.
    for (const label of STATUS_LABELS) {
      expect(screen.getByRole("button", { name: new RegExp(label, "i") })).toBeTruthy();
    }
    // None of Now's own questions ever leak onto this surface.
    expect(screen.queryByRole("button", { name: /which cans/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /this weekend/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /next vacation/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /when is the next race/i })).toBeNull();
  });

  it("shows every gap question as a gap — the manual check's 'gap panes, not an empty screen'", () => {
    render(
      <StatusScreen
        demo={null}
        onScreen={() => {}}
        task={taskState()}
        nowMs={NOW_MS}
        calendarReads={{}}
        calendarConnected={false}
      />,
    );

    // "No answer yet" is the placeholder's collapsed headline, and dormant
    // (a gap) is collapsed by default — the same rule `collapse.ts` gives
    // every other never-answered pane. Asserted against the registry's own
    // count rather than a literal, so a slice that turns one placeholder
    // into a real, non-gap pane (#313 first) shrinks this by exactly one
    // with no edit required here.
    expect(screen.getAllByText("No answer yet")).toHaveLength(STATUS_LABELS.length);
  });
});
