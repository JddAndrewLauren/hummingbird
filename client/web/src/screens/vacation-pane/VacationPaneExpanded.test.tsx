// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import type { BindingDTO, CalendarEventDTO, FreshnessDTO } from "../../store/protocol";
import { fireEvent, render, screen } from "../../test/component";
import { RankedRegion } from "../questions/RankedRegion";
import { EMPTY_QUESTION_SYNC } from "../questions/contract";
import { CALENDAR_REQUEST_KEY, STALE_AFTER_MS } from "./vacation";

// The pane shell's "component tests are the gate" rule (`src/test/component.tsx`): a pure
// module with no caller does not count as done, so everything here mounts the
// REAL `VacationPaneExpanded` through `RankedRegion` — the same path
// `NowScreen` wires in production — rather than unit-testing `vacation.ts`
// twice.

const TRIPS = "trips@g";

const bound: BindingDTO[] = [
  { key: "trips-calendar", known: true, pending: false, value: { state: "text", text: TRIPS } },
];

/** Today, at noon in the device's own zone — "today" is a device-zone
 * question, so anchoring the test to it is what keeps the day counts below
 * true wherever the suite runs. */
const NOW_MS = new Date(new Date().toDateString()).getTime() + 12 * 60 * 60 * 1000;

function daysFromToday(days: number): string {
  return new Date(NOW_MS + days * 86_400_000).toISOString().slice(0, 10);
}

function tripEvent(title: string, startInDays: number, lengthDays: number): CalendarEventDTO {
  const startDate = daysFromToday(startInDays);
  const endExclusive = daysFromToday(startInDays + lengthDays);
  return {
    providerEventId: `evt-${title}`,
    calendarId: TRIPS,
    title,
    when: { kind: "allDay", startDate, endDate: endExclusive },
    recurrenceId: null,
    location: null,
    organizer: null,
    status: "confirmed",
    providerUpdatedAtMs: 0,
    htmlLink: null,
    description: null,
  };
}

function mount(options: {
  events?: CalendarEventDTO[];
  bindings?: BindingDTO[] | null;
  calendarConnected?: boolean;
  freshness?: FreshnessDTO;
  read?: boolean;
  onScreen?: () => void;
}) {
  const freshness = options.freshness ?? { kind: "age", ageMs: 60_000, declaredCadenceMs: null };
  return render(
    <RankedRegion
      surface="now"
      inputs={{
        sync: EMPTY_QUESTION_SYNC,
        bindings: options.bindings === undefined ? bound : options.bindings,
        paneReads: {},
        calendarConnected: options.calendarConnected ?? true,
        calendarReads:
          options.read === false
            ? {}
            : {
                [CALENDAR_REQUEST_KEY]: {
                  state: "read",
                  events: options.events ?? [],
                  freshness,
                },
              },
        items: [],
      }}
      nowMs={NOW_MS}
      syncOutcomeSeq={0}
      onScreen={options.onScreen ?? (() => {})}
    />,
  );
}

/** Clicks this pane's own collapsed row. Targeted by the question's label
 * rather than by its headline: the other registered questions are unbound in
 * these fixtures too and their rows say "Not set up" as well, so a text query
 * would be ambiguous about which pane it opened. */
function expandVacationRow() {
  const row = screen
    .getAllByRole("button")
    .find((button) => button.textContent?.includes("Next vacation"));
  if (row === undefined) {
    throw new Error("no vacation pane row rendered");
  }
  fireEvent.click(row);
}

describe("VacationPaneExpanded (mounted through RankedRegion)", () => {
  it("renders the countdown place-first, with the whole queue under it", () => {
    mount({
      events: [
        tripEvent("Trip: Lisbon", 16, 6),
        tripEvent("Holiday: Oslo", 90, 4),
        tripEvent("Trip: India", 395, 15),
      ],
    });

    // Place and count are separate elements at the same display size, with
    // the joining words between them — the settled verdict's own shape.
    expect(screen.getByText("Lisbon")).toBeTruthy();
    expect(screen.getByText("16")).toBeTruthy();
    expect(screen.getByText("days")).toBeTruthy();

    // The whole queue, never truncated — including the 395-day trip the
    // +90d window could not have answered at all before this slice.
    expect(screen.getByText("Oslo")).toBeTruthy();
    expect(screen.getByText("India")).toBeTruthy();
  });

  it("keeps a trip a year out out of the collapsed-by-default dormant band", () => {
    // ADR-0015 names this pane as the reason "dormant is not a synonym for
    // far away": a `distant` pane is expanded, so the countdown is on screen.
    mount({ events: [tripEvent("Trip: India", 395, 15)] });
    expect(screen.getByText("India")).toBeTruthy();
    expect(screen.getByText("395")).toBeTruthy();
  });

  it("reads mid-trip as a status line with the day count, not a countdown", () => {
    mount({ events: [tripEvent("Trip: Lisbon", -2, 6)] });
    expect(screen.getByText("In Lisbon · day 3 of 6")).toBeTruthy();
    expect(screen.getByText("day 3 of 6")).toBeTruthy();
  });

  it("is still the trip on the day it returns", () => {
    mount({ events: [tripEvent("Trip: Lisbon", -5, 6)] });
    expect(screen.getByText("Home today from Lisbon")).toBeTruthy();
  });

  it("names the horizon when nothing is booked, rather than claiming nothing exists", () => {
    mount({ events: [] });
    // Answered-and-empty is dormant, so the shell collapses it: the row is
    // the answer.
    expect(screen.getByText("Nothing booked in the next 2 years")).toBeTruthy();
  });

  it("still answers when the read is stale, and states its age", () => {
    mount({
      events: [tripEvent("Trip: Lisbon", 16, 6)],
      freshness: { kind: "age", ageMs: STALE_AFTER_MS + 3 * 3_600_000, declaredCadenceMs: null },
    });
    expect(screen.getByText("Lisbon")).toBeTruthy();
    expect(screen.getByText("Stale — as of 27h ago")).toBeTruthy();
  });

  it("never renders an unknown age as fresh", () => {
    mount({ events: [tripEvent("Trip: Lisbon", 16, 6)], freshness: { kind: "unknown" } });
    expect(screen.getByText("Stale — age unknown")).toBeTruthy();
  });

  it("asks for a Trips calendar when none is designated, and routes to Settings", () => {
    const onScreen = vi.fn();
    mount({ bindings: [], onScreen });
    expandVacationRow();
    expect(screen.getByText("Designate a Trips calendar — the countdown reads its events.")).toBeTruthy();
    fireEvent.click(screen.getByText("Open Settings"));
    expect(onScreen).toHaveBeenCalledWith("settings");
  });

  it("asks for a calendar first when none is connected at all", () => {
    mount({ calendarConnected: false, bindings: [] });
    expandVacationRow();
    expect(
      screen.getByText("Connect a calendar, then designate the one your trips live on."),
    ).toBeTruthy();
  });

  it("waits rather than telling a configured reader to set the pane up", () => {
    mount({ read: false });
    expandVacationRow();
    expect(
      screen.getByText("Nothing to count to until this device has read the Trips calendar."),
    ).toBeTruthy();
  });
});
