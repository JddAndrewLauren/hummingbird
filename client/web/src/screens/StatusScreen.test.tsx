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
import { QUESTIONS, rankPanes } from "./questions/registry";
import { realQuestionInputs } from "./NowScreen";
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
    // The expected count is the number of *gap* panes — status panes the
    // registry itself answers `bound-but-unacquired` for these same inputs —
    // not the number of status questions, which stays 4 when #313-#316 each
    // replace a placeholder with a real, non-gap pane. Derived from
    // `rankPanes`, so those slices touch nothing here; the guard keeps this
    // from degenerating into asserting an empty screen.
    const expectedGaps = rankPanes(
      { ...realQuestionInputs(taskState(), {}, false), nowMs: NOW_MS },
      "status",
    ).filter((pane) => pane.answer.answerState === "bound-but-unacquired").length;
    expect(expectedGaps).toBeGreaterThan(0);

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
    // every other never-answered pane. If a gap pane stops rendering its
    // headline, the DOM count falls below `expectedGaps` and this fails.
    expect(screen.getAllByText("No answer yet")).toHaveLength(expectedGaps);
  });
});
