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
import { fireEvent, render, screen, taskState } from "../../test/component";
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

/** Open one tile by the pane it names — the board draws tiles compact until
 * the reader opens one, so a test about what the pane's own body says has to
 * open it first. */
function openTile(name: RegExp): void {
  fireEvent.click(screen.getByRole("button", { name }));
}

describe("GithubPaneExpanded, mounted inside StatusScreen", () => {
  it("renders the never-polled gap when nothing has been read yet", () => {
    render(
      <StatusScreen
        online
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
        online
        task={taskState({ paneReads: { [SOURCE]: rows } })}
        nowMs={NOW_MS}
        calendarReads={{}}
        calendarConnected={false}
      />,
    );

    // Every healthy workflow's sentence is on screen as its tile's own
    // accessible name — the board splits the sentence across the tile's two
    // types, so it is what a screen reader hears rather than one text node.
    for (const name of ["gmail-poll", "calendar-poll", "graph-mail-poll", "graph-calendar-poll"]) {
      expect(
        // Anchored on the label's own dash: a bare `calendar-poll` also
        // matches `graph-calendar-poll`'s tile.
        screen.getByRole("button", { name: new RegExp(`— ${name} · healthy`) }),
      ).toBeTruthy();
    }
    // The stalled one reads as stalled while still compact — no click, no
    // colour-only cue — and opens to its own facts.
    expect(
      screen.getByRole("button", { name: /city-waste · never run/ }),
    ).toBeTruthy();
    openTile(/city-waste · never run/);
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
        online
        task={taskState({ paneReads: { [SOURCE]: rows } })}
        nowMs={NOW_MS}
        calendarReads={{}}
        calendarConnected={false}
      />,
    );

    // Compact, the tile already carries the warning word rather than two
    // unwarned green facts (#314 review round 2's blocker); opened, the body
    // names the unreadable cadence itself.
    expect(
      screen.getByRole("button", { name: /graph-calendar-poll · cadence unreadable/ }),
    ).toBeTruthy();
    openTile(/graph-calendar-poll · cadence unreadable/);
    expect(
      screen.getByText("last run success (schedule), under an hour ago"),
    ).toBeTruthy();
    expect(screen.getByText("cadence unreadable")).toBeTruthy();
  });

  /** The card's `stale — age unknown` arm: a freshness the shell could not
   * age at all. `isStaleFreshness` reads `unknown` as stale, so
   * `githubAnswer` escalates the otherwise-healthy `dormant` reading to
   * `imminent` and the pane opens — which means this card is the whole view
   * the reader gets. Without this line the open card shows two green facts
   * and a "cron stalled" badge with nothing saying *why* it is unjudgeable,
   * and the branch had no test at either level. */
  it("names the staleness without an age when the freshness itself is unknown", () => {
    const rows = read([
      snapshot("gmail-poll.yml", {
        envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ display_name: "gmail-poll" }) },
        freshness: { kind: "unknown" },
      }),
    ]);

    render(
      <StatusScreen
        online
        task={taskState({ paneReads: { [SOURCE]: rows } })}
        nowMs={NOW_MS}
        calendarReads={{}}
        calendarConnected={false}
      />,
    );

    // The staleness is in the tile's own sentence, and the open body says it
    // again in its own words rather than leaving a green-looking fact bare.
    expect(
      screen.getByRole("button", { name: /gmail-poll · answer may be stale/ }),
    ).toBeTruthy();
    openTile(/gmail-poll · answer may be stale/);
    expect(screen.getByText("stale — age unknown")).toBeTruthy();
  });
});
