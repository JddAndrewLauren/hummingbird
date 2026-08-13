// @vitest-environment jsdom

// The wiring gate for ADR-0017/#311's Status screen: three of the four PRs
// in the S10-S13 batch shipped UI state with no reader, and this is exactly
// the check that would have caught it here — a mounted screen, not an
// inspected module, proving the registry's `surface: "status"` filter
// actually reaches the DOM through `StatusScreen` -> `RankedRegion` ->
// `rankPanes`.

import { describe, expect, it } from "vitest";
import { StatusScreen } from "./StatusScreen";
import { render, screen, taskState } from "../test/component";

const NOW_MS = 1_700_000_000_000;

describe("StatusScreen", () => {
  it("renders one pane for every registered status question, and never a 'now' question", () => {
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

    // The four infra placeholders (#311's slice — #313-#316 replace each of
    // these), every one discoverable even though nothing has polled it yet.
    for (const label of ["Kimi balance", "GitHub workflows", "Uptime", "This device"]) {
      expect(screen.getByRole("button", { name: new RegExp(label, "i") })).toBeTruthy();
    }
    // None of Now's own questions ever leak onto this surface.
    expect(screen.queryByRole("button", { name: /which cans/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /this weekend/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /next vacation/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /when is the next race/i })).toBeNull();
  });

  it("shows every infra question as a gap — the manual check's 'four gap panes, not an empty screen'", () => {
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
    // every other never-answered pane.
    expect(screen.getAllByText("No answer yet")).toHaveLength(4);
  });
});
