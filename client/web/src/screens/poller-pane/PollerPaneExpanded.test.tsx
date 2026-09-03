// @vitest-environment jsdom

// #775's own wiring gate, on `UptimePaneExpanded.test.tsx`'s reasoning: a
// mounted screen, not an inspected module, proving a real `context_snapshots`
// row for a watched source reaches the DOM through `StatusScreen` ->
// `StatusBoard` -> the registry -> this pane's body, and that a source with
// no cadence declared never renders as healthy on either the compact tile
// or the open one.

import { describe, expect, it } from "vitest";
import { StatusScreen } from "../StatusScreen";
import type { PaneReadDTO, PaneSnapshotDTO } from "../../store/protocol";
import { fireEvent, render, screen, taskState } from "../../test/component";

const NOW_MS = 1_700_000_000_000;
const KIMI = "kimi-balance/v1";

function snapshot(key: string, overrides: Partial<PaneSnapshotDTO> = {}): PaneSnapshotDTO {
  return {
    key,
    fetchedAtMs: NOW_MS - 60_000,
    envelope: { kind: "ok", schema: KIMI, polledEveryMs: 21_600_000, body: "{}" },
    freshness: { kind: "age", ageMs: 60_000, declaredCadenceMs: 21_600_000 },
    ...overrides,
  };
}

function read(source: string, snapshots: PaneSnapshotDTO[]): PaneReadDTO {
  return { source, snapshots, liveAlerts: [] };
}

function openTile(name: RegExp): void {
  fireEvent.click(screen.getByRole("button", { name }));
}

describe("PollerPaneExpanded, mounted inside StatusScreen", () => {
  it("renders one gap tile per watched source when nothing has been read yet", () => {
    render(
      <StatusScreen
        online
        task={taskState()}
        nowMs={NOW_MS}
        calendarReads={{}}
        calendarConnected={false}
      />,
    );
    // Nine watched sources, each its own gap tile, distinctly named — the
    // whole reason the gap headline carries the source (`poller.ts`'s own
    // header).
    expect(screen.getByRole("button", { name: `Poller freshness — ${KIMI} · No answer yet` })).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "Poller freshness — gmail/v1 · No answer yet" }),
    ).toBeTruthy();
  });

  it("collapses a healthy, recently-written source", () => {
    render(
      <StatusScreen
        online
        task={taskState({ paneReads: { [KIMI]: read(KIMI, [snapshot("balance")]) } })}
        nowMs={NOW_MS}
        calendarReads={{}}
        calendarConnected={false}
      />,
    );
    expect(
      screen.getByRole("button", { name: new RegExp(`${KIMI} · healthy, last row`) }),
    ).toBeTruthy();
  });

  it("opens an overdue source and the card itself names the age and the declared cadence", () => {
    const cadence = 21_600_000;
    const rows = read(KIMI, [
      snapshot("balance", { freshness: { kind: "age", ageMs: cadence * 3 + 60_000, declaredCadenceMs: cadence } }),
    ]);
    render(
      <StatusScreen
        online
        task={taskState({ paneReads: { [KIMI]: rows } })}
        nowMs={NOW_MS}
        calendarReads={{}}
        calendarConnected={false}
      />,
    );
    const toggle = screen.getByRole("button", { name: new RegExp(`${KIMI} · overdue`) });
    expect(toggle).toBeTruthy();
    openTile(new RegExp(`${KIMI} · overdue`));
    expect(screen.getByText("declared cadence 6h")).toBeTruthy();
  });

  /** A source declaring no cadence must never render as healthy, on
   * `poller.rs`'s own rule — the compact tile carries a warn word, not a
   * green one, and the open card names the cadence as unreadable. */
  it("never reads a cadence-less source as healthy", () => {
    const rows = read(KIMI, [
      snapshot("balance", { freshness: { kind: "age", ageMs: 60_000, declaredCadenceMs: null } }),
    ]);
    render(
      <StatusScreen
        online
        task={taskState({ paneReads: { [KIMI]: rows } })}
        nowMs={NOW_MS}
        calendarReads={{}}
        calendarConnected={false}
      />,
    );
    const toggle = screen.getByRole("button", { name: new RegExp(`${KIMI} · cadence unreadable`) });
    expect(toggle.getAttribute("data-tile-tone")).not.toBe("quiet");
    openTile(new RegExp(`${KIMI} · cadence unreadable`));
    expect(screen.getByText("cadence unreadable")).toBeTruthy();
  });
});
