// @vitest-environment jsdom

// #313's own wiring gate, on `StatusScreen.test.tsx`'s reasoning: a mounted
// screen, not an inspected module, proving a real `context_snapshots` row
// reaches the DOM through `StatusScreen` -> `RankedRegion` -> the registry
// -> this pane's `Expanded`, and that the collapsed row answers rather than
// showing the placeholder's "No answer yet" gap now that a poller exists.

import { describe, expect, it } from "vitest";
import { StatusScreen } from "../StatusScreen";
import type { PaneReadDTO, PaneSnapshotDTO } from "../../store/protocol";
import { fireEvent, render, screen, taskState } from "../../test/component";
import { SNAPSHOT_KEY, SOURCE } from "./kimi";

const NOW_MS = 1_700_000_000_000;

function snapshot(overrides: Partial<PaneSnapshotDTO> = {}): PaneSnapshotDTO {
  return {
    key: SNAPSHOT_KEY,
    fetchedAtMs: NOW_MS - 60_000,
    envelope: {
      kind: "ok",
      schema: SOURCE,
      polledEveryMs: 21_600_000,
      body: JSON.stringify({ available_balance: 4.1, voucher_balance: 5.1, cash_balance: -1 }),
    },
    freshness: { kind: "age", ageMs: 60_000, declaredCadenceMs: 21_600_000 },
    ...overrides,
  };
}

function read(snapshots: PaneSnapshotDTO[] = [snapshot()]): PaneReadDTO {
  return { source: SOURCE, snapshots, liveAlerts: [] };
}

/** Open one tile by the pane it names — the board draws tiles compact until
 * the reader opens one, so a test about what the pane's own body says has to
 * open it first. */
function openTile(name: RegExp): void {
  fireEvent.click(screen.getByRole("button", { name }));
}

describe("KimiPaneExpanded, mounted inside StatusScreen", () => {
  // `available_balance: 4.10` bands "near" (`kimiBand`), which is not
  // dormant — `collapse.ts`'s `defaultCollapsed` therefore renders this pane
  // open with no click needed, exactly the "all green is one quiet stack,
  // red announces itself" reading ADR-0017 asks for: a pane worth a second
  // look opens on its own.
  it("renders the answered pane open, not the never-polled placeholder gap", () => {
    render(
      <StatusScreen
        online
        task={taskState({ paneReads: { [SOURCE]: read() } })}
        nowMs={NOW_MS}
        calendarReads={{}}
        calendarConnected={false}
      />,
    );

    const kimiButton = screen.getByRole("button", { name: /kimi balance/i });
    // Asserted on the Kimi tile's own button, not on the whole screen: other
    // status questions can legitimately still be gaps, and only this one must
    // not. Its balance is on the tile compact, with no click needed.
    expect(kimiButton.textContent).not.toMatch(/No answer yet/);
    expect(kimiButton.textContent).toMatch(/\$4\.10 — running low/);
  });

  it("shows the voucher/cash split, flagging the negative cash position", () => {
    render(
      <StatusScreen
        online
        task={taskState({ paneReads: { [SOURCE]: read() } })}
        nowMs={NOW_MS}
        calendarReads={{}}
        calendarConnected={false}
      />,
    );

    openTile(/kimi balance/i);
    expect(screen.getByText(/voucher \$5\.10/)).toBeTruthy();
    expect(screen.getByText(/cash -\$1\.00/)).toBeTruthy();
    expect(screen.getByText("cash owed")).toBeTruthy();
  });

  it("still renders the never-polled gap honestly when no snapshot has landed", () => {
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
});
