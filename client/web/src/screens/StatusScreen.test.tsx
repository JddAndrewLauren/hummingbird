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
import { questionLabel } from "./questions/roster";
import { realQuestionInputs } from "./NowScreen";
import { fireEvent, render, screen, taskState } from "../test/component";
import { StatusBoard } from "./status-board/StatusBoard";

const NOW_MS = 1_700_000_000_000;

/** The reachability tile, whichever arm it is drawn as. An answered
 * reachability pane has nothing to disclose beneath its headline, so its
 * tile is a plain `div` with no toggle — a role query would miss it.
 *
 * Found by the question's name out of the core's roster (#714), not by a
 * literal: the reachability pane's headline carries no subject of its own,
 * so its label IS the tile's bold line, and spelling it here would be a
 * second copy of the one string the roster exists to hold. */
function reachabilityTile(): Element {
  const name = questionLabel("reachability").toLowerCase();
  const tile = [...document.querySelectorAll("[data-tile-tone]")].find((node) =>
    (node.getAttribute("aria-label") ?? "").toLowerCase().includes(name),
  );
  if (!tile) throw new Error("no reachability tile on screen");
  return tile;
}

// Derived from the registry and the core's roster (#714), not hardcoded, so
// a later status question does not require a second label roster in its
// component test. Asserting non-empty keeps this from degenerating into a
// tautology against an empty registry.
//
// A function rather than a module-level constant: `questionLabel` reads the
// decision seam, and module scope runs before `initDecisions()` has
// resolved.
function statusLabels(): string[] {
  return QUESTION_ORDER.filter((q) => QUESTIONS[q].surface === "status").map((q) =>
    questionLabel(q),
  );
}

describe("StatusScreen", () => {
  it("renders one pane for every registered status question, and never a 'now' question", () => {
    const labels = statusLabels();
    expect(labels.length).toBeGreaterThan(0);

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
    // has acquired any answer. `getAllBy*`, not `getBy*`: every question but
    // poller (#775) collapses to one sentinel subject on a fresh device —
    // poller always ranks one gap tile per source it watches, so its own
    // label matches several buttons at once.
    for (const label of labels) {
      expect(
        screen.getAllByRole("button", { name: new RegExp(label, "i") }).length,
      ).toBeGreaterThan(0);
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
    expect(reachabilityTile().getAttribute("data-tile-tone")).toBe("quiet");

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
    expect(reachabilityTile().getAttribute("data-tile-tone")).toBe("danger");
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
