// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import type { BindingDTO } from "../../store/protocol";
import { fireEvent, render, screen } from "../../test/component";
import { EMPTY_QUESTION_SYNC, type QuestionInputs } from "../questions/contract";
import { RankedRegion } from "../questions/RankedRegion";
import { BINDING_KEY, lockTime } from "./fantasy";
import { FantasyPaneExpanded } from "./FantasyPaneExpanded";
import { GOLDEN_INVALID, GOLDEN_VALID, HOUR, THURSDAY, lineupWorld } from "./golden-lineup.testkit";

// Both no-data arms, on screen: the collapsed rows through the real
// `RankedRegion` (the path `NowScreen` wires), and the expanded body mounted
// directly, since a dormant pane rests collapsed.

const NOW = new Date(2026, 8, 20, 9, 0, 0).getTime();

function world(bindings: BindingDTO[]): QuestionInputs {
  return {
    sync: EMPTY_QUESTION_SYNC,
    bindings,
    paneReads: {},
    calendarReads: {},
    calendarConnected: false,
    items: [],
    nowMs: NOW,
  };
}

function leagues(text: string): BindingDTO {
  return { key: BINDING_KEY, known: true, pending: false, value: { state: "text", text } };
}

function mountRegion(inputs: QuestionInputs) {
  render(
    <RankedRegion
      surface="now"
      inputs={inputs}
      nowMs={inputs.nowMs}
      syncOutcomeSeq={1}
      storage={{ getItem: () => null, setItem: () => {}, removeItem: () => {} }}
      onScreen={vi.fn()}
    />,
  );
}

describe("the fantasy panes", () => {
  it("unbound: both rows are on Now, each saying it is not set up", () => {
    mountRegion(world([]));
    expect(screen.getByText("Lineup · Not set up")).toBeDefined();
    expect(screen.getByText("Waivers · Not set up")).toBeDefined();
  });

  it("two leagues: four never-polled rows", () => {
    mountRegion(world([leagues("449.l.1,449.l.2")]));
    expect(screen.getAllByText(/· Never polled$/).filter((row) => /League [12] ·/.test(row.textContent ?? ""))).toHaveLength(4);
  });

  it("unbound, expanded: the setup prompt routes to Settings", () => {
    const onSetupNavigate = vi.fn();
    render(<FantasyPaneExpanded subjectKey="setup:waivers" inputs={world([])} onSetupNavigate={onSetupNavigate} />);
    expect(screen.getByText("Which leagues do you play in?")).toBeDefined();
    fireEvent.click(screen.getByRole("button", { name: /Open Settings/ }));
    expect(onSetupNavigate).toHaveBeenCalledOnce();
  });

  it("bound, expanded: names the league and the obligation, and says it was never polled", () => {
    render(<FantasyPaneExpanded subjectKey="449.l.123456:lineup" inputs={world([leagues("449.l.123456")])} />);
    expect(screen.getByText("League 123456 · Lineup")).toBeDefined();
    expect(screen.getByText("This league's lineup has not been polled yet.")).toBeDefined();
  });
});

// #855: an answered lineup, on screen — the rendered tile through the real
// `RankedRegion`, over the poller's own golden bodies.
describe("the lineup tile", () => {
  it("valid: rests collapsed on the core's headline, and opens onto every starting slot", () => {
    const inputs = lineupWorld(GOLDEN_VALID, THURSDAY - 2 * 24 * HOUR);
    mountRegion(inputs);
    const headline = `Twinion Family League · Lineup set — locks ${lockTime(THURSDAY, inputs.nowMs)}`;
    const row = screen.getByRole("button", { name: new RegExp(headline.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")) });
    expect(row.getAttribute("aria-expanded")).toBe("false");
    expect(screen.queryByText("Thor Thursday")).toBeNull();

    fireEvent.click(row);
    expect(screen.getByText("Twinion Family League · Week 4")).toBeDefined();
    expect(screen.getByText(`Lineup set — locks ${lockTime(THURSDAY, inputs.nowMs)}`)).toBeDefined();
    const slots = screen.getByRole("list", { name: "Gridiron Hummingbirds starting slots" });
    expect(slots.querySelectorAll("li")).toHaveLength(8);
    expect(screen.getByText("Thor Thursday")).toBeDefined();
    // A questionable starter is valid, and still says so.
    expect(screen.getByText(/· Questionable/)).toBeDefined();
    expect(screen.queryByText(/^Fix by/)).toBeNull();
  });

  it("invalid: opens by itself onto each invalid slot's occupant, reason and replacement deadline", () => {
    const inputs = lineupWorld(GOLDEN_INVALID, THURSDAY - 4 * HOUR);
    mountRegion(inputs);
    // Imminent panes rest open: the headline is the card's own.
    expect(screen.getByText("3 slots to fix — locks in 4h")).toBeDefined();
    expect(screen.getByText("Wade Byeman")).toBeDefined();
    expect(screen.getByText("On bye")).toBeDefined();
    expect(screen.getByText("No starter")).toBeDefined();
    expect(screen.getByText("Empty")).toBeDefined();
    expect(screen.getByText("Owen Sidelined")).toBeDefined();
    expect(screen.getAllByText(`Fix by ${lockTime(THURSDAY, inputs.nowMs)}`)).toHaveLength(2);
    expect(screen.getByText(`Fix by ${lockTime(1_791_133_200_000, inputs.nowMs)}`)).toBeDefined();
  });

  it("stale in crunch time: the headline says when it was last checked", () => {
    render(
      <FantasyPaneExpanded
        subjectKey="449.l.123456:lineup"
        inputs={lineupWorld(GOLDEN_INVALID, THURSDAY - 4 * HOUR, 3 * HOUR)}
      />,
    );
    expect(screen.getByText("3 slots to fix, last checked 3h ago — locks in 4h")).toBeDefined();
  });

  it("past the general rule: keeps showing the answer and says how old it is", () => {
    render(
      <FantasyPaneExpanded
        subjectKey="449.l.123456:lineup"
        inputs={lineupWorld(GOLDEN_VALID, THURSDAY - 3 * 24 * HOUR, 13 * HOUR)}
      />,
    );
    expect(screen.getByText("stale — as of 13h ago")).toBeDefined();
    expect(screen.getByText("Thor Thursday")).toBeDefined();
  });
});
