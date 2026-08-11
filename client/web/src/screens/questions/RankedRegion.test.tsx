// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import { BINDING_KEY, SOURCE } from "../waste-pane/waste";
import type { BindingDTO, PaneReadDTO } from "../../store/protocol";
import { fireEvent, paneReadDTO, paneSnapshotDTO, render, screen, wasteBody } from "../../test/component";
import type { StorageLike } from "./collapse";
import { RankedRegion } from "./RankedRegion";

// The wiring gate for #245's region. Every piece below is separately
// unit-tested and separately right; what these tests cover is the thread —
// that the captured order really is captured, that the collapsed row really
// is the shell's, and that a stored override really survives a remount.

const ZONE = "America/Los_Angeles";
/** Monday 2026-08-10, 09:00 at the address. */
const NOW = Date.parse("2026-08-10T16:00:00Z");

function civilDate(dayOffset: number): string {
  return new Intl.DateTimeFormat("en-CA", {
    timeZone: ZONE,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(new Date(NOW + dayOffset * 86_400_000));
}

function bound(): BindingDTO[] {
  return [
    {
      key: BINDING_KEY,
      known: true,
      pending: false,
      value: { state: "text", text: "https://example.gov/collection" },
    },
  ];
}

function readAt(dayOffset: number): Record<string, PaneReadDTO> {
  const day = civilDate(dayOffset);
  return {
    [SOURCE]: paneReadDTO({
      snapshots: [
        paneSnapshotDTO({
          envelope: {
            kind: "ok",
            schema: SOURCE,
            polledEveryMs: 86_400_000,
            body: wasteBody({ zone: ZONE, scheduled: day, collectedOn: day }),
          },
        }),
      ],
    }),
  };
}

function fakeStorage(): StorageLike & { entries: Map<string, string> } {
  const entries = new Map<string, string>();
  return {
    entries,
    getItem: (key) => entries.get(key) ?? null,
    setItem: (key, value) => void entries.set(key, value),
    removeItem: (key) => void entries.delete(key),
  };
}

describe("RankedRegion", () => {
  it("renders an answered, imminent pane expanded, from real-shaped inputs", () => {
    render(
      <RankedRegion
        inputs={{ bindings: bound(), paneReads: readAt(1), calendarReads: {}, calendarConnected: false, items: [] }}
        nowMs={NOW}
        syncOutcomeSeq={0}
        onScreen={() => {}}
      />,
    );

    expect(screen.getByText("Trash Tonight")).toBeTruthy();
    // The bins carry accessible names — colour alone is not a label.
    expect(screen.getByLabelText("trash")).toBeTruthy();
  });

  it("draws a collapsed pane itself, with no pane markup at all", () => {
    // Four days out is dormant, and dormant IS the collapsed row: there is
    // no quiet card, and the pane ships no compact form of its own.
    render(
      <RankedRegion
        inputs={{ bindings: bound(), paneReads: readAt(4), calendarReads: {}, calendarConnected: false, items: [] }}
        nowMs={NOW}
        syncOutcomeSeq={0}
        onScreen={() => {}}
      />,
    );

    expect(screen.getByText("Friday · 4d")).toBeTruthy();
    expect(screen.queryByText("Trash Friday")).toBeNull();
    // `weekend`'s own row is ALSO collapsed here (an unrequested calendar
    // read is `bound-but-unacquired`, dormant) — scoped by name to the
    // waste pane specifically, the same "no pane markup at all" proof.
    expect(screen.getByRole("button", { name: /which cans/i, expanded: false })).toBeTruthy();
  });

  it("does not move a pane when only the band changed, but does update what it shows", () => {
    // The whole point of capturing the order: the clock moving a pane from
    // dormant to imminent must not re-sort the region under the reader's
    // cursor — while the content it renders stays live.
    const view = render(
      <RankedRegion
        inputs={{ bindings: bound(), paneReads: readAt(4), calendarReads: {}, calendarConnected: false, items: [] }}
        nowMs={NOW}
        syncOutcomeSeq={0}
        onScreen={() => {}}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /which cans/i, expanded: false }));
    expect(screen.getByText("Trash Friday")).toBeTruthy();

    view.rerender(
      <RankedRegion
        inputs={{ bindings: bound(), paneReads: readAt(1), calendarReads: {}, calendarConnected: false, items: [] }}
        nowMs={NOW}
        syncOutcomeSeq={0}
        onScreen={() => {}}
      />,
    );

    // Live content, no re-sort needed to see it.
    expect(screen.getByText("Trash Tonight")).toBeTruthy();
  });

  it("re-samples the order when a cycle completes", () => {
    const view = render(
      <RankedRegion
        inputs={{ bindings: bound(), paneReads: readAt(4), calendarReads: {}, calendarConnected: false, items: [] }}
        nowMs={NOW}
        syncOutcomeSeq={0}
        onScreen={() => {}}
      />,
    );
    expect(screen.getByText("Friday · 4d")).toBeTruthy();

    view.rerender(
      <RankedRegion
        inputs={{ bindings: bound(), paneReads: readAt(1), calendarReads: {}, calendarConnected: false, items: [] }}
        nowMs={NOW}
        syncOutcomeSeq={1}
        onScreen={() => {}}
      />,
    );

    // Re-ranked on the new facts: imminent, so it opens on its own.
    expect(screen.getByText("Trash Tonight")).toBeTruthy();
  });

  it("re-samples at once when a pane's answer state changes, without waiting for a cycle", () => {
    // A fresh mount whose bindings land a moment later must not sit in the
    // wrong answer state for a whole minute — and while the table is unread
    // it says so, rather than telling a configured reader to set it up.
    // `calendarConnected: true` throughout — this test is about the waste
    // question's own unread-bindings state, and a weekend-pane "Not set up"
    // row (unbound only via `!calendarConnected`, #122 review fix) would
    // make the "Not set up" assertion ambiguous. Both assertions are scoped
    // to the waste row for the same reason since #119: the race pane reads
    // the same unread `bindings` table and says the same words about it.
    const view = render(
      <RankedRegion
        inputs={{ bindings: null, paneReads: {}, calendarReads: {}, calendarConnected: true, items: [] }}
        nowMs={NOW}
        syncOutcomeSeq={0}
        onScreen={() => {}}
      />,
    );
    const wasteRow = () => screen.getByRole("button", { name: /which cans/i });
    expect(wasteRow().textContent).toContain("Checking setup");
    expect(wasteRow().textContent).not.toContain("Not set up");

    view.rerender(
      <RankedRegion
        inputs={{ bindings: bound(), paneReads: readAt(1), calendarReads: {}, calendarConnected: true, items: [] }}
        nowMs={NOW}
        syncOutcomeSeq={0}
        onScreen={() => {}}
      />,
    );

    expect(screen.getByText("Trash Tonight")).toBeTruthy();
  });

  it("keeps a collapse override across a remount, through storage", () => {
    const storage = fakeStorage();
    const props = {
      inputs: { bindings: bound(), paneReads: readAt(1), calendarReads: {}, calendarConnected: false, items: [] },
      nowMs: NOW,
      syncOutcomeSeq: 0,
      storage,
      onScreen: () => {},
    };
    const view = render(<RankedRegion {...props} />);

    // Imminent, so it starts open; collapse it.
    fireEvent.click(screen.getByRole("button", { expanded: true }));
    expect(screen.queryByText("Trash Tonight")).toBeNull();

    view.unmount();
    render(<RankedRegion {...props} />);

    expect(screen.queryByText("Trash Tonight")).toBeNull();
    expect(screen.getByText("Trash tonight")).toBeTruthy();
  });

  it("renders a gap as a gap, with the reason in words", () => {
    render(
      <RankedRegion
        inputs={{
          bindings: bound(),
          paneReads: {
            [SOURCE]: paneReadDTO({
              snapshots: [
                paneSnapshotDTO({ envelope: { kind: "malformed", reason: "`body` is missing" } }),
              ],
            }),
          },
          calendarReads: {}, calendarConnected: false, items: [],
        }}
        nowMs={NOW}
        syncOutcomeSeq={0}
        onScreen={() => {}}
      />,
    );

    expect(screen.getByText("No answer yet")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /which cans/i, expanded: false }));
    expect(screen.getByText(/`body` is missing/)).toBeTruthy();
  });

  it("tells an unread bindings table apart from an unset binding, on screen", () => {
    render(
      <RankedRegion
        inputs={{ bindings: null, paneReads: {}, calendarReads: {}, calendarConnected: false, items: [] }}
        nowMs={NOW}
        syncOutcomeSeq={0}
        onScreen={() => {}}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /which cans/i, expanded: false }));
    expect(screen.getByText("Checking your setup")).toBeTruthy();
    // The one thing it must not do: tell someone who has already configured
    // this to configure it.
    expect(screen.queryByText("Open Settings")).toBeNull();
  });

  it("offers Settings when the binding exists but cannot be read", () => {
    const onScreen = vi.fn();
    render(
      <RankedRegion
        inputs={{
          bindings: [{ key: BINDING_KEY, known: true, pending: false, value: { state: "other", raw: "7" } }],
          paneReads: {},
          calendarReads: {}, calendarConnected: false, items: [],
        }}
        nowMs={NOW}
        syncOutcomeSeq={0}
        onScreen={onScreen}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /which cans/i, expanded: false }));
    expect(screen.getByText("That collection page can't be read")).toBeTruthy();
    fireEvent.click(screen.getByText("Open Settings"));
    expect(onScreen).toHaveBeenCalledWith("settings");
  });

  it("sorts an unbound question last and offers its setup prompt", () => {
    const onScreen = vi.fn();
    render(
      <RankedRegion
        // `calendarConnected: true` keeps the weekend pane out of its own
        // `unbound` state here — this test is about the waste question's
        // unset binding, and a second "Not set up" row from the weekend
        // pane (unbound only via `!calendarConnected`, #122 review fix)
        // would make "the" unbound row ambiguous. The `trips-calendar`
        // binding does the same job for #121's vacation pane, whose second
        // unbound reason is an undesignated Trips calendar. The race pane's
        // own unset binding still says it too (#119), so assertions target
        // the waste question by name rather than by the shared headline.
        inputs={{
          bindings: [
            {
              key: "trips-calendar",
              known: true,
              pending: false,
              value: { state: "text", text: "trips@g" },
            },
          ],
          paneReads: {},
          calendarReads: {},
          calendarConnected: true,
          items: [],
        }}
        nowMs={NOW}
        syncOutcomeSeq={0}
        onScreen={onScreen}
      />,
    );

    // Every unbound question sinks below the one that merely has no data
    // yet (the weekend pane, connected but unpolled here) — the sort's first
    // axis, with two unbound questions in the world since #119.
    const rows = screen.getAllByRole("button");
    expect(rows[rows.length - 1].textContent).toContain("Not set up");
    expect(rows[0].textContent).not.toContain("Not set up");

    fireEvent.click(screen.getByRole("button", { name: /which cans/i }));
    fireEvent.click(screen.getByText("Open Settings"));
    expect(onScreen).toHaveBeenCalledWith("settings");
  });
});
