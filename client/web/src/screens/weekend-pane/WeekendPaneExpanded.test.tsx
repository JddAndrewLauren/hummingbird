// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import type { TaskItemDTO } from "../../store/protocol";
import { fireEvent, render, screen } from "../../test/component";
import { RankedRegion } from "../questions/RankedRegion";
import { CALENDAR_REQUEST_KEY, weekendWindow } from "./weekend";

// The pane shell's "component tests are the gate" rule (CLAUDE.md): a pure
// module with no caller does not count as done, so this mounts the REAL
// `WeekendPaneExpanded` through `RankedRegion` — the same path `NowScreen`
// wires in production — and exercises the do-date chip affordance rather
// than merely unit-testing `weekend.ts`'s pure functions.

function item(overrides: Partial<TaskItemDTO> & { id: string }): TaskItemDTO {
  return {
    id: overrides.id,
    seq: null,
    title: overrides.title ?? overrides.id,
    description: null,
    stage: "ready",
    size: null,
    energy: null,
    context: null,
    priority: 2,
    projectId: null,
    projectPos: null,
    deadline: overrides.deadline ?? null,
    scheduledDate: overrides.scheduledDate ?? null,
    source: null,
    sourceKey: null,
    sourceUrl: null,
    archivedAt: null,
    createdAt: 0,
    updatedAt: 0,
    version: 1,
    pending: false,
  };
}

// An hour into a live weekend, so the pane starts expanded (`live` band,
// `answered` state) without needing to click through the collapsed row.
const LIVE_NOW_MS = weekendWindow(Date.now()).startMs + 60 * 60 * 1000;
const SATURDAY_KEY = weekendWindow(LIVE_NOW_MS).days[1].key;

describe("WeekendPaneExpanded (mounted through RankedRegion)", () => {
  it("renders due and scheduled entries distinctly, and a due item's residue do-date", () => {
    render(
      <RankedRegion
        inputs={{
          bindings: [],
          paneReads: {},
          calendarReads: {
            [CALENDAR_REQUEST_KEY]: { state: "read", events: [], freshness: { kind: "unknown" } },
          },
          items: [
            item({ id: "due-1", title: "Renew the car insurance", deadline: SATURDAY_KEY }),
            item({ id: "sched-1", title: "Repot the olive tree", scheduledDate: SATURDAY_KEY }),
          ],
        }}
        nowMs={LIVE_NOW_MS}
        syncOutcomeSeq={0}
        onScreen={() => {}}
      />,
    );

    expect(screen.getByText("Renew the car insurance")).toBeTruthy();
    expect(screen.getByText("Repot the olive tree")).toBeTruthy();
    // Distinct semantically: the due row's meta line never says "anytime"
    // (the scheduled row's own time label), and vice versa.
    expect(screen.getByText(/by end of day/)).toBeTruthy();
    expect(screen.getByText(/anytime/)).toBeTruthy();
  });

  it("an item both due and scheduled the same day renders exactly once, as due, do-date shown", () => {
    render(
      <RankedRegion
        inputs={{
          bindings: [],
          paneReads: {},
          calendarReads: {
            [CALENDAR_REQUEST_KEY]: { state: "read", events: [], freshness: { kind: "unknown" } },
          },
          items: [
            item({ id: "both", title: "File the taxes", deadline: SATURDAY_KEY, scheduledDate: SATURDAY_KEY }),
          ],
        }}
        nowMs={LIVE_NOW_MS}
        syncOutcomeSeq={0}
        onScreen={() => {}}
      />,
    );

    expect(screen.getAllByText("File the taxes")).toHaveLength(1);
    expect(screen.getByText(/by end of day.*planned sat/i)).toBeTruthy();
  });

  it("clicking an unplanned day chip on a due row sets that item's do-date", () => {
    const onSetScheduledDate = vi.fn();
    render(
      <RankedRegion
        inputs={{
          bindings: [],
          paneReads: {},
          calendarReads: {
            [CALENDAR_REQUEST_KEY]: { state: "read", events: [], freshness: { kind: "unknown" } },
          },
          items: [item({ id: "due-1", title: "Renew the car insurance", deadline: SATURDAY_KEY })],
        }}
        nowMs={LIVE_NOW_MS}
        syncOutcomeSeq={0}
        onScreen={() => {}}
        onSetScheduledDate={onSetScheduledDate}
      />,
    );

    fireEvent.click(screen.getByTitle(/plan it for sat/i));

    expect(onSetScheduledDate).toHaveBeenCalledWith("due-1", SATURDAY_KEY);
  });

  it("clicking the already-planned chip clears the do-date", () => {
    const onSetScheduledDate = vi.fn();
    render(
      <RankedRegion
        inputs={{
          bindings: [],
          paneReads: {},
          calendarReads: {
            [CALENDAR_REQUEST_KEY]: { state: "read", events: [], freshness: { kind: "unknown" } },
          },
          items: [item({ id: "sched-1", title: "Repot the olive tree", scheduledDate: SATURDAY_KEY })],
        }}
        nowMs={LIVE_NOW_MS}
        syncOutcomeSeq={0}
        onScreen={() => {}}
        onSetScheduledDate={onSetScheduledDate}
      />,
    );

    fireEvent.click(screen.getByTitle(/clear this do-date/i));

    expect(onSetScheduledDate).toHaveBeenCalledWith("sched-1", null);
  });

  it("renders a clear-weekend empty state for a genuinely empty answered pane", () => {
    render(
      <RankedRegion
        inputs={{
          bindings: [],
          paneReads: {},
          calendarReads: {
            [CALENDAR_REQUEST_KEY]: { state: "read", events: [], freshness: { kind: "unknown" } },
          },
          items: [],
        }}
        nowMs={LIVE_NOW_MS}
        syncOutcomeSeq={0}
        onScreen={() => {}}
      />,
    );

    expect(screen.getByText("A clear weekend")).toBeTruthy();
  });

  it("offers Settings when no calendar has ever been connected", () => {
    const onScreen = vi.fn();
    render(
      <RankedRegion
        inputs={{
          bindings: [],
          paneReads: {},
          calendarReads: { [CALENDAR_REQUEST_KEY]: { state: "not_read" } },
          items: [],
        }}
        nowMs={LIVE_NOW_MS}
        syncOutcomeSeq={0}
        onScreen={onScreen}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /this weekend/i, expanded: false }));
    expect(screen.getByText("What are my plans this weekend?")).toBeTruthy();
    fireEvent.click(screen.getByText("Open Settings"));
    expect(onScreen).toHaveBeenCalledWith("settings");
  });
});
