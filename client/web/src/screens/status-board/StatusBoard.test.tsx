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
import { tileParts } from "./tile-copy";

const NOW_MS = 1_700_000_000_000;

/** Every tile, in DOM order — `data-tile-tone` is on both the openable and
 * the plain arm, which is what it is for. */
function tilesInOrder(): string[] {
  return [...document.querySelectorAll("[data-tile-tone]")].map(
    (tile) => tile.getAttribute("aria-label") ?? "",
  );
}

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
      const label = QUESTIONS[pane.question].label;
      const { name, fact } = tileParts(label, pane.answer.collapsedHeadline);
      // The exact string, not a permissive regex: a `tileParts` that never
      // split would still produce something a `label.*headline` pattern
      // matched, so the old version of this test could not fail against the
      // bug it was meant to catch.
      const spoken =
        name === label ? `${label} — ${fact}` : `${label} — ${name} · ${fact}`;
      expect(screen.getByLabelText(spoken)).toBeTruthy();
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

  // Position is a function of identity, never of the band — the claim
  // ADR-0033 makes. The old version of this test clicked a tile and compared
  // DOM order, which cannot fail: nothing in the render path derives order
  // from `expandedKey`. This one moves a pane's band between renders, which
  // is the input the claim is actually about.
  it("keeps every tile where it was when a pane's band changes", () => {
    const early = taskState();
    const { rerender } = render(board(early));
    // In DOM order, and across both arms: a tile is a `button` when it has
    // something to disclose and a `div` when it has not, and reachability
    // crosses that line as soon as it has an answer — so a role-by-role
    // query would report a move the board never made.
    const before = tilesInOrder();

    // A newer, failed cycle re-bands reachability (the same move
    // `StatusScreen.test.tsx` uses to turn a tile from quiet to danger).
    rerender(
      board(
        taskState({
          lastSyncOutcome: {
            kind: "pull_failed",
            retryAfterMs: null,
            activeItemCount: 0,
            wasFullSweep: false,
            deadLettered: 0,
          },
          lastSyncAtMs: NOW_MS - 30_000,
          lastSuccessfulSyncAtMs: NOW_MS - 7 * 60_000,
          syncOutcomeSeq: 2,
        }),
      ),
    );

    const after = tilesInOrder();
    // The reachability tile's own sentence moved (that is the point), so
    // compare the tiles' *identities* rather than their words.
    expect(after.length).toBe(before.length);
    expect(after.map((label) => label?.split(" — ")[0])).toEqual(
      before.map((label) => label?.split(" — ")[0]),
    );
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

  // The unpolled board — the state ADR-0033 calls the honest first frame,
  // and the one this redesign shipped without ever rendering. Every defect
  // the wrap-up review found on the web side lived here.
  describe("the board nobody has polled yet", () => {
    it("says an opened gap's sentence once, not twice", () => {
      render(board());
      const gaps = ranked().filter(
        (pane) => pane.answer.answerState !== "answered",
      );
      expect(gaps.length).toBeGreaterThan(0);

      // **Every** gap tile, not the first. The first is Kimi, whose gap
      // heading ("No balance answer yet") happens to differ from its
      // sentence ("No answer yet") — so it is the one pane on which this
      // duplication cannot show, and checking only it proved nothing.
      let checked = 0;
      for (const gap of gaps) {
        const sentence = gap.answer.collapsedHeadline;
        // By the tile's whole accessible name, not by the sentence: three of
        // the four gap panes share the sentence "No answer yet", so a
        // substring match kept returning the same tile and this loop silently
        // checked one pane three times over.
        const paneLabel = QUESTIONS[gap.question].label;
        const { name, fact } = tileParts(paneLabel, sentence);
        const spoken =
          name === paneLabel
            ? `${paneLabel} — ${fact}`
            : `${paneLabel} — ${name} · ${fact}`;
        const toggle = screen.queryByLabelText(spoken);
        if (!toggle || toggle.getAttribute("aria-expanded") === null) continue;
        fireEvent.click(toggle);
        const detail = document.querySelector(".hb-status-tile-detail");
        expect(detail).toBeTruthy();
        // The tile's header already carries the sentence; the body under it
        // must add the reason, not repeat the heading.
        expect(detail?.textContent ?? "").not.toContain(sentence);
        checked += 1;
      }
      expect(checked).toBeGreaterThan(0);
    });

    it("gives an opened gap tile a reason, never an empty card", () => {
      render(board());
      const toggle = screen
        .getAllByRole("button")
        .find((tile) => tile.getAttribute("data-tile-tone") === "gap");
      expect(toggle).toBeTruthy();
      fireEvent.click(toggle as HTMLElement);
      const detail = document.querySelector(".hb-status-tile-detail");
      expect(detail).toBeTruthy();
      // More than the mono name line the header already said.
      expect((detail?.textContent ?? "").trim().length).toBeGreaterThan(
        (toggle?.getAttribute("aria-label") ?? "").length / 4,
      );
    });
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
