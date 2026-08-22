// @vitest-environment jsdom

// The wiring gate for ADR-0017/#311's Status screen: three of the four PRs
// in the S10-S13 batch shipped UI state with no reader, and this is exactly
// the check that would have caught it here — a mounted screen, not an
// inspected module, proving the registry's `surface: "status"` filter
// actually reaches the DOM through `StatusScreen` -> `StatusBoard` ->
// `rankPanes`.

import { describe, expect, it } from "vitest";
import { StatusScreen } from "./StatusScreen";
import { QUESTION_ORDER } from "./questions/contract";
import { QUESTIONS, rankPanes } from "./questions/registry";
import { realQuestionInputs } from "./NowScreen";
import { fireEvent, render, screen, taskState } from "../test/component";
import { StatusBoard } from "./status-board/StatusBoard";

const NOW_MS = 1_700_000_000_000;

// Derived from the registry, not hardcoded, so a later status question does
// not require a second label roster in its component test. Asserting
// non-empty keeps this from degenerating into a tautology against an empty
// registry.
const STATUS_LABELS = QUESTION_ORDER.filter(
  (q) => QUESTIONS[q].surface === "status",
).map((q) => QUESTIONS[q].label);

describe("StatusScreen", () => {
  it("renders one pane for every registered status question, and never a 'now' question", () => {
    expect(STATUS_LABELS.length).toBeGreaterThan(0);

    render(
      <StatusScreen
        online
        task={taskState()}
        nowMs={NOW_MS}
        calendarReads={{}}
        calendarConnected={false}
      />,
    );

    // Every registered status question is discoverable before this device
    // has acquired any answer.
    for (const label of STATUS_LABELS) {
      expect(
        screen.getByRole("button", { name: new RegExp(label, "i") }),
      ).toBeTruthy();
    }
    // None of Now's own questions ever leak onto this surface.
    expect(screen.queryByRole("button", { name: /which cans/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /this weekend/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /next vacation/i })).toBeNull();
    expect(
      screen.queryByRole("button", { name: /when is the next race/i }),
    ).toBeNull();
  });

  it("shows every gap question as a gap — the manual check's 'gap panes, not an empty screen'", () => {
    // The expected count is the number of *gap* panes — status panes the
    // registry itself answers `bound-but-unacquired` for these same inputs —
    // not the number of status questions. Derived from `rankPanes`, so the
    // guard stays accurate as question implementations change without
    // degenerating into asserting an empty screen.
    const expectedGaps = rankPanes(
      { ...realQuestionInputs(taskState(), {}, false), nowMs: NOW_MS },
      "status",
    ).filter(
      (pane) => pane.answer.answerState === "bound-but-unacquired",
    ).length;
    expect(expectedGaps).toBeGreaterThan(0);

    render(
      <StatusScreen
        online
        task={taskState()}
        nowMs={NOW_MS}
        calendarReads={{}}
        calendarConnected={false}
      />,
    );

    // Poller-backed gaps retain the shared headline; the device-local pane's
    // explicit never-synced wording is also a gap and must count rather than
    // disappear just because it does not use the generic sentence.
    expect(
      screen.getAllByText(/^(No answer yet|Never synced on this device\.)$/),
    ).toHaveLength(expectedGaps);
  });

  it("turns a tile from quiet to alarming when a newer cycle makes its success stale", () => {
    const completed = {
      kind: "completed" as const,
      retryAfterMs: null,
      activeItemCount: 2,
      wasFullSweep: false,
      deadLettered: 0,
    };
    const recentAtMs = NOW_MS - 60_000;
    const { rerender } = render(
      <StatusScreen
        online
        task={taskState({
          lastSyncOutcome: completed,
          lastSyncAtMs: recentAtMs,
          lastSuccessfulSyncAtMs: recentAtMs,
          syncOutcomeSeq: 1,
        })}
        nowMs={NOW_MS}
        calendarReads={{}}
        calendarConnected={false}
      />,
    );

    expect(screen.getByText("Synced 1m ago")).toBeTruthy();
    expect(
      screen
        .getByRole("button", { name: /this device/i })
        .getAttribute("data-tile-tone"),
    ).toBe("quiet");

    rerender(
      <StatusScreen
        online
        task={taskState({
          lastSyncOutcome: { ...completed, kind: "pull_failed" },
          lastSyncAtMs: NOW_MS - 30_000,
          lastSuccessfulSyncAtMs: NOW_MS - 7 * 60_000,
          syncOutcomeSeq: 2,
        })}
        nowMs={NOW_MS}
        calendarReads={{}}
        calendarConnected={false}
      />,
    );

    // The board reads content live, so the tile's own sentence moves with the
    // cycle. What replaced the old auto-open assertion: a tile no longer
    // opens itself when its answer turns bad — single selection belongs to
    // the reader — so what must change is the tile's *treatment*, and a
    // change that showed up only in colour would leave this green.
    expect(screen.getByText("Last synced 7m ago")).toBeTruthy();
    expect(
      screen
        .getByRole("button", { name: /this device/i })
        .getAttribute("data-tile-tone"),
    ).toBe("danger");
  });

  it("opens one tile at a time, and remembers which through a remount", () => {
    const storage = new Map<string, string>();
    const stub = {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => void storage.set(key, value),
      removeItem: (key: string) => void storage.delete(key),
    };
    const board = (
      <StatusBoard
        inputs={realQuestionInputs(taskState(), {}, false)}
        nowMs={NOW_MS}
        online
        queueDepth={null}
        lastSyncOutcome={null}
        lastSyncAtMs={null}
        storage={stub}
      />
    );

    const { unmount } = render(board);
    const tiles = screen.getAllByRole("button");
    expect(tiles.length).toBeGreaterThan(1);

    fireEvent.click(tiles[0]);
    expect(tiles[0].getAttribute("aria-expanded")).toBe("true");

    // Opening a second closes the first: one open tile is the whole state.
    fireEvent.click(screen.getAllByRole("button")[1]);
    const after = screen.getAllByRole("button");
    expect(
      after.filter((tile) => tile.getAttribute("aria-expanded") === "true"),
    ).toHaveLength(1);

    const openName = after
      .find((tile) => tile.getAttribute("aria-expanded") === "true")
      ?.getAttribute("aria-label");
    unmount();
    render(board);
    expect(
      screen
        .getByRole("button", { name: openName as string })
        .getAttribute("aria-expanded"),
    ).toBe("true");
  });
});
