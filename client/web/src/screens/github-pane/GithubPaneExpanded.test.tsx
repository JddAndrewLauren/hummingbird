// @vitest-environment jsdom

// #314's own wiring gate, on `KimiPaneExpanded.test.tsx`'s reasoning: a
// mounted screen, not an inspected module, proving a real `context_snapshots`
// row per scheduled workflow reaches the DOM through `StatusScreen` ->
// `RankedRegion` -> the registry -> this pane's `Expanded`, and that a
// healthy stack of several workflows collapses while a stalled one does not
// — the brief's own "the main test of whether the collapsed stack stays
// readable" with five-plus workflows.

import { describe, expect, it } from "vitest";
import { StatusScreen } from "../StatusScreen";
import type { PaneReadDTO, PaneSnapshotDTO } from "../../store/protocol";
import { render, screen, taskState } from "../../test/component";
import { SOURCE } from "./github";

const NOW_MS = 1_700_000_000_000;

function body(overrides: Record<string, unknown> = {}): string {
  return JSON.stringify({
    display_name: "workflow",
    declared_cadence_ms: 15 * 60 * 1000,
    last_run_conclusion: "success",
    last_run_event: "schedule",
    last_run_at_ms: NOW_MS - 60_000,
    last_scheduled_success_at_ms: NOW_MS - 60_000,
    ...overrides,
  });
}

function snapshot(key: string, overrides: Partial<PaneSnapshotDTO> = {}): PaneSnapshotDTO {
  return {
    key,
    fetchedAtMs: NOW_MS - 60_000,
    envelope: { kind: "ok", schema: SOURCE, polledEveryMs: 86_400_000, body: body() },
    freshness: { kind: "age", ageMs: 60_000, declaredCadenceMs: 86_400_000 },
    ...overrides,
  };
}

function read(snapshots: PaneSnapshotDTO[]): PaneReadDTO {
  return { source: SOURCE, snapshots, liveAlerts: [] };
}

describe("GithubPaneExpanded, mounted inside StatusScreen", () => {
  it("renders the never-polled gap when nothing has been read yet", () => {
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
    expect(screen.getAllByText("No answer yet").length).toBeGreaterThan(0);
  });

  it("collapses a stack of healthy workflows and lifts a stalled one open", () => {
    const rows = read([
      snapshot("gmail-poll.yml", {
        envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ display_name: "gmail-poll" }) },
      }),
      snapshot("calendar-poll.yml", {
        envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ display_name: "calendar-poll" }) },
      }),
      snapshot("graph-mail-poll.yml", {
        envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ display_name: "graph-mail-poll" }) },
      }),
      snapshot("graph-calendar-poll.yml", {
        envelope: {
          kind: "ok",
          schema: SOURCE,
          polledEveryMs: null,
          body: body({ display_name: "graph-calendar-poll" }),
        },
      }),
      // The fifth, stalled workflow — never run at all.
      snapshot("city-waste.yml", {
        envelope: {
          kind: "ok",
          schema: SOURCE,
          polledEveryMs: null,
          body: body({
            display_name: "city-waste",
            last_run_conclusion: null,
            last_run_event: null,
            last_run_at_ms: null,
            last_scheduled_success_at_ms: null,
          }),
        },
      }),
    ]);

    render(
      <StatusScreen
        demo={null}
        onScreen={() => {}}
        task={taskState({ paneReads: { [SOURCE]: rows } })}
        nowMs={NOW_MS}
        calendarReads={{}}
        calendarConnected={false}
      />,
    );

    // Every healthy workflow's collapsed sentence is on screen — the row is
    // collapsed, but the collapsed row's own headline still renders.
    for (const name of ["gmail-poll", "calendar-poll", "graph-mail-poll", "graph-calendar-poll"]) {
      expect(screen.getByText(`${name} · healthy`)).toBeTruthy();
    }
    // The stalled one opens on its own (`live` is not dormant, so
    // `collapse.ts`'s default rule lifts it) and shows its own expanded
    // content — a collapsed row renders no expanded content at all
    // (`RankedRegion`'s own split), so finding this proves the row opened.
    expect(screen.getByText("city-waste")).toBeTruthy();
    expect(screen.getByText("never run")).toBeTruthy();
    expect(screen.getByText("no scheduled success on record")).toBeTruthy();
  });

  it("opens a cadence-unreadable workflow and the card itself says the judgement could not be made", () => {
    // `declared_cadence_ms: null` bands `distant`, which is non-`dormant`,
    // so `collapse.ts`'s default rule opens the pane — the reader gets this
    // card, never the collapsed headline. The card must therefore carry the
    // "cadence unreadable" fact itself; without its `distant` arm this pane
    // renders two green facts unwarned (#314 review round 2's blocker).
    const rows = read([
      snapshot("graph-calendar-poll.yml", {
        envelope: {
          kind: "ok",
          schema: SOURCE,
          polledEveryMs: null,
          body: body({ display_name: "graph-calendar-poll", declared_cadence_ms: null }),
        },
      }),
    ]);

    render(
      <StatusScreen
        demo={null}
        onScreen={() => {}}
        task={taskState({ paneReads: { [SOURCE]: rows } })}
        nowMs={NOW_MS}
        calendarReads={{}}
        calendarConnected={false}
      />,
    );

    // The expanded card is what rendered (a collapsed row renders no
    // expanded content at all), and it names the unreadable cadence.
    expect(screen.getByText("graph-calendar-poll")).toBeTruthy();
    expect(screen.getByText("last run success (schedule), under an hour ago")).toBeTruthy();
    expect(screen.getByText("cadence unreadable")).toBeTruthy();
    // The collapsed sentence is NOT what matched above — this pane is open.
    expect(screen.queryByText(/· cadence unreadable/)).toBeNull();
  });
});
