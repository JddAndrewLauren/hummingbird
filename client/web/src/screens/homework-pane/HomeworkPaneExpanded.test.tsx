// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import type { TaskItemDTO } from "../../store/protocol";
import { render, screen } from "../../test/component";
import { EMPTY_QUESTION_SYNC, type QuestionInputs } from "../questions/contract";
import { RankedRegion } from "../questions/RankedRegion";

// The pane shell's "component tests are the gate" rule
// (`src/test/component.tsx`): a pure module with no caller compiles, passes
// and does nothing, so this mounts the REAL `HomeworkPaneExpanded` through
// `RankedRegion` — the path `NowScreen` wires in production — and reads what
// actually lands on screen.
//
// #675's whole reason for existing is that the preparation notes are
// reachable without going to find the item, so the assertion that matters
// most here is that the notes are *on screen*.

const NOW = new Date(2026, 7, 21, 9, 0, 0).getTime();
const CONTEXT = "@homework";

function inDays(days: number): string {
  const at = new Date(NOW);
  const then = new Date(at.getFullYear(), at.getMonth(), at.getDate() + days);
  const month = String(then.getMonth() + 1).padStart(2, "0");
  const day = String(then.getDate()).padStart(2, "0");
  return `${then.getFullYear()}-${month}-${day}`;
}

function item(overrides: Partial<TaskItemDTO> & { id: string }): TaskItemDTO {
  return {
    seq: null,
    title: overrides.id,
    description: null,
    stage: "ready",
    size: null,
    energy: null,
    context: CONTEXT,
    priority: 2,
    projectId: null,
    projectPos: null,
    deadline: null,
    scheduledDate: null,
    source: null,
    sourceKey: null,
    sourceUrl: null,
    archivedAt: null,
    createdAt: 0,
    updatedAt: 0,
    version: 1,
    pending: false,
    ...overrides,
  };
}

function world(items: TaskItemDTO[]): Omit<QuestionInputs, "nowMs"> {
  return {
    sync: EMPTY_QUESTION_SYNC,
    bindings: [],
    paneReads: {},
    calendarReads: {},
    calendarConnected: false,
    items,
  };
}

function mount(items: TaskItemDTO[]) {
  render(
    <RankedRegion
      surface="now"
      inputs={world(items)}
      nowMs={NOW}
      syncOutcomeSeq={1}
      storage={{ getItem: () => null, setItem: () => {}, removeItem: () => {} }}
      onScreen={vi.fn()}
    />,
  );
}

describe("HomeworkPaneExpanded (mounted through RankedRegion)", () => {
  it("puts the winning item's title and its notes on screen", () => {
    mount([
      item({
        id: "essay",
        title: "Prep for Thursday's session",
        description: "read chapter 4\nbring the printout",
        deadline: inDays(1),
      }),
    ]);

    expect(screen.getByText("Homework due tomorrow")).toBeDefined();
    expect(screen.getByText("Prep for Thursday's session")).toBeDefined();
    // The notes verbatim, line breaks and all — this is the one surface
    // that reads them.
    expect(screen.getByText(/read chapter 4/)).toBeDefined();
    expect(screen.getByText(/bring the printout/)).toBeDefined();
  });

  it("lists the other open items beneath the winner, and counts them", () => {
    mount([
      item({ id: "soon", title: "Soonest", deadline: inDays(1) }),
      item({ id: "later", title: "Later one", deadline: inDays(6) }),
      item({ id: "undated", title: "No date on it" }),
    ]);

    expect(screen.getByText("2 more open")).toBeDefined();
    expect(screen.getByText("Later one")).toBeDefined();
    expect(screen.getByText("No date on it")).toBeDefined();
  });

  it("never shows an item in another context, or a finished one", () => {
    mount([
      item({ id: "mine", title: "Mine", deadline: inDays(1) }),
      item({ id: "garden", title: "Not homework", context: "@garden", deadline: inDays(0) }),
      item({ id: "finished", title: "Already handed in", stage: "done", deadline: inDays(0) }),
    ]);

    expect(screen.getByText("Mine")).toBeDefined();
    expect(screen.queryByText("Not homework")).toBeNull();
    expect(screen.queryByText("Already handed in")).toBeNull();
  });

  it("reads a captured, untriaged item — the widened items union (#675)", () => {
    mount([
      item({ id: "captured", title: "Just captured", stage: "triage", deadline: inDays(2) }),
    ]);

    expect(screen.getByText("Just captured")).toBeDefined();
  });
});
