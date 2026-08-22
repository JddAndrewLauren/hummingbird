// @vitest-environment jsdom

// #315's own wiring gate, on `GithubPaneExpanded.test.tsx`'s reasoning: a
// mounted screen, not an inspected module, proving a real `context_snapshots`
// row per declared service reaches the DOM through `StatusScreen` ->
// `StatusBoard` -> the registry -> this pane's body, and that a probe that
// cannot tell the truth never renders as healthy on either the compact tile
// or the open one.
//
// Two things the board changed about how this reads. A compact tile splits
// the pane's collapsed sentence into its two types, so the whole sentence is
// no longer one text node — it is the tile button's accessible name, which is
// what these tests now assert (and what a screen reader actually hears). And
// a tile stays compact until the reader opens it, so a test about what the
// pane's own body says has to open it first.

import { describe, expect, it } from "vitest";
import { StatusScreen } from "../StatusScreen";
import type { PaneReadDTO, PaneSnapshotDTO } from "../../store/protocol";
import { fireEvent, render, screen, taskState } from "../../test/component";
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

/** Open one tile by the pane it names. */
function openTile(name: RegExp): void {
  fireEvent.click(screen.getByRole("button", { name }));
}

describe("UptimePaneExpanded, mounted inside StatusScreen", () => {
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

  it("collapses a healthy authority/web/runner stack", () => {
    const rows = read([
      snapshot("authority", { envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ expect_status: 401, observed_status: 401 }) } }),
      snapshot("web", { envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ expect_status: 200, observed_status: 200 }) } }),
      snapshot("runner", { envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ expect_status: 401, observed_status: 401 }) } }),
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

    expect(screen.getByRole("button", { name: /authority · 401 as expected/ })).toBeTruthy();
    expect(screen.getByRole("button", { name: /web · 200 as expected/ })).toBeTruthy();
    expect(screen.getByRole("button", { name: /runner · 401 as expected/ })).toBeTruthy();
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
        online
        task={taskState({ paneReads: { [SOURCE]: rows } })}
        nowMs={NOW_MS}
        calendarReads={{}}
        calendarConnected={false}
      />,
    );

    // Compact, the fault is already a coloured glance word rather than a
    // healthy-looking one; opened, the body names the transport error itself.
    expect(screen.getByRole("button", { name: /runner · unreachable/ })).toBeTruthy();
    openTile(/runner · unreachable/);
    expect(screen.getByText("unreachable — connection refused")).toBeTruthy();
    expect(screen.getByText("unreachable")).toBeTruthy();
  });

  it("opens an expected-off service that unexpectedly answers, and the card names the fault the other way", () => {
    const rows = read([
      snapshot("runner", { envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ expected: "off", expect_status: 401, observed_status: 401, error: null }) } }),
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

    openTile(/runner · reachable/);
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
        online
        task={taskState({ paneReads: { [SOURCE]: rows } })}
        nowMs={NOW_MS}
        calendarReads={{}}
        calendarConnected={false}
      />,
    );

    expect(
      screen.getByRole("button", { name: /runner · off, as expected/ }),
    ).toBeTruthy();
  });

  /** The card's `near` arm — an `expected: "on"` service whose door is open
   * but answering with the wrong code. Reachable-but-wrong is the *lesser*
   * fault (`uptimeBand`'s own reasoning), so it is the arm most at risk of
   * being read as fine; the card has to name it. */
  it("opens a reachable-but-wrong-status service and the card names the unexpected status", () => {
    const rows = read([
      snapshot("authority", { envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ expect_status: 401, observed_status: 500 }) } }),
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

    // The lesser fault still has to be named rather than read as fine: the
    // compact tile says so, and the open one shows the raw observation.
    expect(
      screen.getByRole("button", { name: /authority · unexpected status/ }),
    ).toBeTruthy();
    openTile(/authority · unexpected status/);
    expect(screen.getByText("answered 500 (wanted 401)")).toBeTruthy();
    expect(screen.getByText("unexpected status")).toBeTruthy();
  });

  /** The card's staleness line, with an age to report. A `dormant` reading
   * this old is escalated to `imminent` by `uptimeAnswer`, so the pane
   * opens — and the card must repeat the staleness, or an open pane shows a
   * green-looking observation with nothing saying it is hours out of date. */
  it("names the staleness on the card when the probe workflow itself has gone quiet", () => {
    const rows = read([
      snapshot("web", {
        envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ expect_status: 200, observed_status: 200 }) },
        freshness: { kind: "age", ageMs: 5 * 60 * 60 * 1000, declaredCadenceMs: 3_600_000 },
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

    openTile(/web ·/);
    expect(screen.getByText("stale — as of 5h ago")).toBeTruthy();
  });

  /** The other half of the same line: a freshness the shell could not age at
   * all. `isStaleFreshness` reads `unknown` as stale, so the pane still
   * opens; the card must say so without inventing an age. */
  it("names the staleness without an age when the freshness itself is unknown", () => {
    const rows = read([
      snapshot("web", {
        envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ expect_status: 200, observed_status: 200 }) },
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

    openTile(/web ·/);
    expect(screen.getByText("stale — age unknown")).toBeTruthy();
  });

  it("reads a malformed payload as a gap, never as a healthy reading", () => {
    const rows = read([
      snapshot("authority", { envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: "not json at all" } }),
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

    expect(screen.getAllByText("No answer yet").length).toBeGreaterThan(0);
  });
});
