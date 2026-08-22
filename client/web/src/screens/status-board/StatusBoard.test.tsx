// @vitest-environment jsdom

// The board's own wiring gate: that the panes `rankPanes` decided reach the
// DOM as two labelled grids, that a tile's treatment follows the decided
// answer rather than anything computed here, and that position does not move
// when a band does.

import { describe, expect, it } from "vitest";
import { realQuestionInputs } from "../NowScreen";
import { QUESTIONS, rankPanes } from "../questions/registry";
import { QUESTION_ORDER } from "../questions/contract";
import { fireEvent, render, screen, taskState } from "../../test/component";
import { StatusBoard } from "./StatusBoard";

const NOW_MS = 1_700_000_000_000;

function board(task = taskState()) {
  return (
    <StatusBoard
      inputs={realQuestionInputs(task, {}, false)}
      nowMs={NOW_MS}
      online
      queueDepth={null}
      lastSyncOutcome={task.lastSyncOutcome}
      lastSyncAtMs={task.lastSyncAtMs}
    />
  );
}

/** Called inside each test, never at module scope: the decision seam throws
 * if it is reached before `initDecisions()` has resolved (ADR-0025 as amended
 * by #500/#501), and a module-level `const` is evaluated at import time. */
function ranked() {
  return rankPanes(
    { ...realQuestionInputs(taskState(), {}, false), nowMs: NOW_MS },
    "status",
  );
}

describe("StatusBoard", () => {
  it("draws exactly one toggle per ranked pane, and no other", () => {
    render(board());
    expect(ranked().length).toBeGreaterThan(0);
    expect(screen.getAllByRole("button")).toHaveLength(ranked().length);
    expect(document.querySelectorAll("[aria-expanded]")).toHaveLength(
      ranked().length,
    );
  });

  it("labels each group with the number of subjects actually in it", () => {
    render(board());
    const infra = ranked().filter((pane) =>
      ["kimi", "uptime", "reachability"].includes(pane.question),
    ).length;
    expect(infra).toBeGreaterThan(0);
    expect(
      screen.getByText(new RegExp(`^infra · ${infra} subjects?$`)),
    ).toBeTruthy();
  });

  it("names every pane in a tile's own accessible name", () => {
    render(board());
    for (const pane of ranked()) {
      expect(
        screen.getByRole("button", {
          name: new RegExp(
            `${QUESTIONS[pane.question].label}.*${escape(pane.answer.collapsedHeadline)}`,
          ),
        }),
      ).toBeTruthy();
    }
  });

  it("opens a tile on click and closes it on a second", () => {
    render(board());
    const tile = screen.getAllByRole("button")[0];
    expect(tile.getAttribute("aria-expanded")).toBe("false");
    fireEvent.click(tile);
    expect(screen.getAllByRole("button")[0].getAttribute("aria-expanded")).toBe(
      "true",
    );
    fireEvent.click(screen.getAllByRole("button")[0]);
    expect(screen.getAllByRole("button")[0].getAttribute("aria-expanded")).toBe(
      "false",
    );
  });

  // Position is a function of identity, never of the band — which is the
  // whole reason this board needs no captured sample. Opening a tile must
  // not reorder the board either.
  it("keeps every tile where it was when one of them opens", () => {
    render(board());
    const before = screen
      .getAllByRole("button")
      .map((tile) => tile.getAttribute("aria-label"));
    fireEvent.click(screen.getAllByRole("button")[1]);
    const after = screen
      .getAllByRole("button")
      .map((tile) => tile.getAttribute("aria-label"));
    expect(after).toEqual(before);
  });

  it("orders each group by the declared question order, not by salience", () => {
    render(board());
    const labelOrder = QUESTION_ORDER.filter(
      (q) => QUESTIONS[q].surface === "status",
    ).map((q) => QUESTIONS[q].label);
    // Per grid, not across the board: grouping comes first, and the declared
    // order is what orders the tiles inside one group.
    for (const grid of document.querySelectorAll(".hb-status-grid")) {
      const drawn = [...grid.querySelectorAll("button")].map(
        (tile) => tile.getAttribute("aria-label")?.split(" — ")[0] ?? "",
      );
      const firstSeen = drawn.filter(
        (label, at) => drawn.indexOf(label) === at,
      );
      expect(firstSeen).toEqual(
        labelOrder.filter((label) => firstSeen.includes(label)),
      );
    }
  });

  // A gap is not "as expected": it must never wear the healthy dot.
  it("gives a gap tile no green dot", () => {
    render(board());
    const gaps = screen
      .getAllByRole("button")
      .filter((tile) => tile.getAttribute("data-tile-tone") === "gap");
    expect(gaps.length).toBeGreaterThan(0);
    for (const tile of gaps) {
      expect(tile.querySelector(".hb-status-dot")).toBeNull();
    }
  });
});

function escape(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
