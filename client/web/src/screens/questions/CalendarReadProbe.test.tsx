// @vitest-environment jsdom

import { describe, expect, it } from "vitest";
import type { CalendarEventDTO, CalendarReadDTO } from "../../store/protocol";
import { render, screen } from "../../test/component";
import type { QuestionInputs } from "./contract";
import { CalendarReadProbe, describeCalendarRead } from "./CalendarReadProbe";

// Issue #267's acceptance: "a component test mounts something that consumes
// the new arm and asserts it renders" — this is that test. It also carries
// the two states the brief calls out as independently distinguishable
// ("not read yet" vs "no events in the interval") and the all-day
// recoverability proof, since all three are consumer-side facts about the
// wire shape this probe reads.

function inputsWith(calendarReads: Record<string, CalendarReadDTO | undefined>): QuestionInputs {
  return { bindings: [], paneReads: {}, calendarReads, nowMs: 1_000 };
}

const MEETING: CalendarEventDTO = {
  providerEventId: "evt-1",
  calendarId: "cal-primary",
  title: "Standup",
  start: { instantMs: 1_000, timeZone: "America/Los_Angeles" },
  end: { instantMs: 2_000, timeZone: "America/Los_Angeles" },
  allDay: false,
  recurrenceId: null,
  location: null,
  organizer: null,
  status: "confirmed",
  providerUpdatedAtMs: 900,
  htmlLink: null,
};

describe("describeCalendarRead", () => {
  it("says nothing was requested when the map has no entry at all", () => {
    expect(describeCalendarRead(undefined)).toBe("Not requested yet");
  });

  it("distinguishes never-synced from a real, empty read", () => {
    // The acceptance criterion this pins: "'Not read yet' is distinguishable
    // from 'no events in the interval'".
    expect(describeCalendarRead({ state: "not_read" })).toBe(
      "This device has never synced its calendar",
    );
    expect(
      describeCalendarRead({ state: "read", events: [], freshness: { kind: "unknown" } }),
    ).toBe("No events in range");
  });

  it("names the events a real read carries", () => {
    expect(
      describeCalendarRead({ state: "read", events: [MEETING], freshness: { kind: "unknown" } }),
    ).toBe("Standup");
  });
});

describe("CalendarReadProbe", () => {
  it("renders the not-requested-yet state for a missing key", () => {
    render(<CalendarReadProbe requestKey="weekend" inputs={inputsWith({})} />);
    expect(screen.getByTestId("calendar-read-probe").textContent).toBe("Not requested yet");
  });

  it("renders the not_read state, distinct from an empty read", () => {
    render(
      <CalendarReadProbe
        requestKey="weekend"
        inputs={inputsWith({ weekend: { state: "not_read" } })}
      />,
    );
    expect(screen.getByTestId("calendar-read-probe").textContent).toBe(
      "This device has never synced its calendar",
    );
  });

  it("renders a real empty read as no events, not as not-read", () => {
    render(
      <CalendarReadProbe
        requestKey="weekend"
        inputs={inputsWith({
          weekend: { state: "read", events: [], freshness: { kind: "unknown" } },
        })}
      />,
    );
    expect(screen.getByTestId("calendar-read-probe").textContent).toBe("No events in range");
  });

  it("renders a real read's events, reading only its own request key", () => {
    render(
      <CalendarReadProbe
        requestKey="weekend"
        inputs={inputsWith({
          weekend: { state: "read", events: [MEETING], freshness: { kind: "unknown" } },
          today: { state: "read", events: [], freshness: { kind: "unknown" } },
        })}
      />,
    );
    expect(screen.getByTestId("calendar-read-probe").textContent).toBe("Standup");
  });

  it("recovers an all-day event's civil date from the DTO alone, in the calendar's own zone", () => {
    // The defect ADR-0015 records on #121: flattening a boundary to the
    // device's own day is wrong the moment the reader is in a different
    // zone from the calendar. The DTO must carry enough to resolve the
    // CORRECT civil date without guessing the device's zone at all.
    const allDay: CalendarEventDTO = {
      ...MEETING,
      title: "Public Holiday",
      allDay: true,
      // Local midnight Aug 10 in Auckland, which is still Aug 9 evening in
      // Los Angeles — the exact case a device-local flattening gets wrong.
      start: { instantMs: Date.parse("2026-08-09T12:00:00Z"), timeZone: "Pacific/Auckland" },
      end: { instantMs: Date.parse("2026-08-10T12:00:00Z"), timeZone: "Pacific/Auckland" },
    };

    const civilDate = new Intl.DateTimeFormat("en-CA", {
      timeZone: allDay.start.timeZone,
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
    }).format(new Date(allDay.start.instantMs));

    expect(civilDate).toBe("2026-08-10");
    // And the wrong reading — resolving the same instant in some other
    // zone, the way a device-local flattening would — genuinely differs,
    // which is exactly why both `instantMs` and `timeZone` have to cross.
    const deviceLocalGuess = new Intl.DateTimeFormat("en-CA", {
      timeZone: "America/Los_Angeles",
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
    }).format(new Date(allDay.start.instantMs));
    expect(deviceLocalGuess).not.toBe(civilDate);
  });
});
