// @vitest-environment jsdom

import { describe, expect, it } from "vitest";
import type { BindingDTO, CalendarEventDTO, FreshnessDTO } from "../../store/protocol";
import { fireEvent, render, screen } from "../../test/component";
import { RankedRegion } from "../questions/RankedRegion";
import { EMPTY_QUESTION_SYNC } from "../questions/contract";
import { CALENDAR_REQUEST_KEY, STALE_AFTER_MS } from "./scps";

// The pane shell's "component tests are the gate" rule: everything here
// mounts the REAL `ScpsPaneExpanded` through `RankedRegion` — `NowScreen`'s
// own path — on `VacationPaneExpanded.test.tsx`'s precedent.

/** Today, at noon in the device's own zone. */
const NOW_MS = new Date(new Date().toDateString()).getTime() + 12 * 60 * 60 * 1000;

function atHour(daysFromToday: number, hour: number): number {
  return NOW_MS - 12 * 60 * 60 * 1000 + daysFromToday * 86_400_000 + hour * 60 * 60 * 1000;
}

function scpsEvent(
  id: string,
  title: string,
  startInDays: number,
  startHour: number,
  endHour: number,
): CalendarEventDTO {
  return {
    providerEventId: id,
    calendarId: "primary",
    title,
    when: { kind: "timed", startMs: atHour(startInDays, startHour), endMs: atHour(startInDays, endHour) },
    recurrenceId: null,
    location: "The clubhouse",
    organizer: null,
    status: "confirmed",
    providerUpdatedAtMs: 0,
    htmlLink: null,
    description: "Bring your own gear.",
  };
}

function mount(options: {
  events?: CalendarEventDTO[];
  bindings?: BindingDTO[];
  calendarConnected?: boolean;
  freshness?: FreshnessDTO;
  read?: boolean;
}) {
  const freshness = options.freshness ?? { kind: "age", ageMs: 60_000, declaredCadenceMs: null };
  return render(
    <RankedRegion
      surface="now"
      inputs={{
        sync: EMPTY_QUESTION_SYNC,
        bindings: options.bindings ?? [],
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
      onScreen={() => {}}
    />,
  );
}

function expandScpsRow() {
  const row = screen
    .getAllByRole("button")
    .find((button) => button.textContent?.includes("Next SCPS event"));
  if (row === undefined) {
    throw new Error("no scps pane row rendered");
  }
  fireEvent.click(row);
}

describe("ScpsPaneExpanded (mounted through RankedRegion)", () => {
  it("renders the next event's kind, time, location and notes", () => {
    mount({ events: [scpsEvent("e1", "SCPS Activity: Tide Pools", 3, 9, 12)] });
    expect(screen.getByText(/SCPS Activity/)).toBeTruthy();
    expect(screen.getByText("The clubhouse")).toBeTruthy();
    expect(screen.getByText("Bring your own gear.")).toBeTruthy();
  });

  it("lists further SCPS events beneath the next one, never truncated", () => {
    mount({
      events: [
        scpsEvent("e1", "SCPS Meeting: A", 0, 14, 16),
        scpsEvent("e2", "SCPS Activity: B", 5, 9, 12),
        scpsEvent("e3", "SCPS Happy Hour", 10, 17, 18),
      ],
    });
    expect(screen.getByText(/SCPS Meeting today/)).toBeTruthy();
    expect(screen.getByText(/SCPS Activity/)).toBeTruthy();
    expect(screen.getByText(/SCPS Happy Hour/)).toBeTruthy();
  });

  it("names the horizon's dormant state when nothing is scheduled, on hand click", () => {
    mount({ events: [] });
    expandScpsRow();
    expect(screen.getByText("No SCPS event scheduled.")).toBeTruthy();
  });

  it("still answers when the read is stale, and states its age", () => {
    mount({
      events: [scpsEvent("e1", "SCPS Meeting: X", 1, 14, 16)],
      freshness: { kind: "age", ageMs: STALE_AFTER_MS + 3 * 3_600_000, declaredCadenceMs: null },
    });
    expect(screen.getByText("Stale — as of 27h ago")).toBeTruthy();
  });

  it("waits rather than claiming an answer before the first calendar sync", () => {
    mount({ read: false });
    expandScpsRow();
    expect(
      screen.getByText("Nothing to show until this device has read its calendars."),
    ).toBeTruthy();
  });

  it("shows the current month's Photo Quest phrase", () => {
    const today = new Date(NOW_MS).toISOString().slice(0, 7);
    mount({
      events: [],
      bindings: [
        {
          key: "scps-quest",
          known: true,
          pending: false,
          value: { state: "text", text: `${today} Reflected Light` },
        },
      ],
    });
    expandScpsRow();
    expect(screen.getByText("Photo Quest — Reflected Light")).toBeTruthy();
  });
});
