// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import type { BindingDTO } from "../../store/protocol";
import { fireEvent, render, screen } from "../../test/component";
import { EMPTY_QUESTION_SYNC, type QuestionInputs } from "../questions/contract";
import { RankedRegion } from "../questions/RankedRegion";
import { BINDING_KEY } from "./fantasy";
import { FantasyPaneExpanded } from "./FantasyPaneExpanded";

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
      nowMs={NOW}
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
