// @vitest-environment jsdom

// #315's own wiring gate, on `GithubPaneExpanded.test.tsx`'s reasoning: a
// mounted screen, not an inspected module, proving a real `context_snapshots`
// row per declared service reaches the DOM through `StatusScreen` ->
// `RankedRegion` -> the registry -> this pane's `Expanded`, and that a
// probe that cannot tell the truth never renders as healthy on either the
// collapsed row or the expanded card.

import { describe, expect, it } from "vitest";
import { StatusScreen } from "../StatusScreen";
import type { PaneReadDTO, PaneSnapshotDTO } from "../../store/protocol";
import { render, screen, taskState } from "../../test/component";
import { SOURCE } from "./uptime";

const NOW_MS = 1_700_000_000_000;

function body(overrides: Record<string, unknown> = {}): string {
  return JSON.stringify({
    expected: "on",
    expect_status: 401,
    observed_status: 401,
    error: null,
    ...overrides,
  });
}

function snapshot(key: string, overrides: Partial<PaneSnapshotDTO> = {}): PaneSnapshotDTO {
  return {
    key,
    fetchedAtMs: NOW_MS - 5 * 60_000,
    envelope: { kind: "ok", schema: SOURCE, polledEveryMs: 3_600_000, body: body() },
    freshness: { kind: "age", ageMs: 5 * 60_000, declaredCadenceMs: 3_600_000 },
    ...overrides,
  };
}

function read(snapshots: PaneSnapshotDTO[]): PaneReadDTO {
  return { source: SOURCE, snapshots, liveAlerts: [] };
}

describe("UptimePaneExpanded, mounted inside StatusScreen", () => {
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

  it("collapses a healthy authority/web/runner stack", () => {
    const rows = read([
      snapshot("authority", { envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ expect_status: 401, observed_status: 401 }) } }),
      snapshot("web", { envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ expect_status: 200, observed_status: 200 }) } }),
      snapshot("runner", { envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ expect_status: 401, observed_status: 401 }) } }),
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

    expect(screen.getByText("authority · 401 as expected")).toBeTruthy();
    expect(screen.getByText("web · 200 as expected")).toBeTruthy();
    expect(screen.getByText("runner · 401 as expected")).toBeTruthy();
  });

  /** The class of bug that sank #314 twice: an unparseable/unreachable
   * reading must never fall through to a healthy-looking band on the
   * collapsed row OR the expanded card. */
  it("opens an unreachable expected-on service and the card itself names the transport error", () => {
    const rows = read([
      snapshot("runner", { envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ expect_status: 401, observed_status: null, error: "connection refused" }) } }),
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

    // The collapsed sentence must NOT be what rendered — this pane is open,
    // proven by finding the card's own runner heading (a collapsed row
    // renders no expanded content at all, `RankedRegion`'s own split).
    expect(screen.queryByText(/runner · unreachable/)).toBeNull();
    expect(screen.getByText("runner")).toBeTruthy();
    expect(screen.getByText("unreachable — connection refused")).toBeTruthy();
    expect(screen.getByText("unreachable")).toBeTruthy();
  });

  it("opens an expected-off service that unexpectedly answers, and the card names the fault the other way", () => {
    const rows = read([
      snapshot("runner", { envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ expected: "off", expect_status: 401, observed_status: 401, error: null }) } }),
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

    expect(screen.getByText("runner")).toBeTruthy();
    expect(screen.getByText("reachable when it should be off")).toBeTruthy();
  });

  /** ADR-0017 decision 4's own agreement case: an expected-off service
   * correctly unreachable collapses quietly rather than reading as a
   * status word. */
  it("collapses an expected-off service that is correctly unreachable, reading as agreement not a status word", () => {
    const rows = read([
      snapshot("runner", { envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ expected: "off", expect_status: 401, observed_status: null, error: "connection refused" }) } }),
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

    expect(screen.getByText("runner · off, as expected")).toBeTruthy();
  });

  it("reads a malformed payload as a gap, never as a healthy reading", () => {
    const rows = read([
      snapshot("authority", { envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: "not json at all" } }),
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

    expect(screen.getAllByText("No answer yet").length).toBeGreaterThan(0);
  });
});
