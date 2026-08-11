import { describe, expect, it } from "vitest";
import type { BindingDTO, CalendarEventDTO, CalendarReadDTO } from "../../store/protocol";
import type { QuestionInputs } from "../questions/contract";
import {
  CALENDAR_REQUEST_KEY,
  HORIZON_LABEL,
  STALE_AFTER_MS,
  isStaleFreshness,
  tripName,
  tripQueue,
  vacationAnswer,
  vacationBand,
  vacationHeadline,
  vacationView,
} from "./vacation";

// Every date below is written as a civil date in a named zone, never as an
// epoch number computed by hand — the pane's whole contract is that it reads
// civil dates and never subtracts instants.

/** An all-day event, the provider's own shape: `start` at local midnight of
 * the first day and `end` at local midnight **after** the last day, both
 * anchored in `zone`. */
function allDay(
  id: string,
  title: string,
  startDate: string,
  endExclusiveDate: string,
  zone = "Europe/Lisbon",
  calendarId = "trips@g",
): CalendarEventDTO {
  const at = (date: string) => new Date(`${date}T00:00:00${offsetOf(date, zone)}`).getTime();
  return {
    providerEventId: id,
    calendarId,
    title,
    start: { instantMs: at(startDate), timeZone: zone },
    end: { instantMs: at(endExclusiveDate), timeZone: zone },
    allDay: true,
    recurrenceId: null,
    location: null,
    organizer: null,
    status: "confirmed",
    providerUpdatedAtMs: 0,
    htmlLink: null,
  };
}

/** The UTC offset `zone` was on at local midnight of `date`, as `+HH:MM` —
 * derived from `Intl` rather than hardcoded, so a DST case in the tests is a
 * real one. */
function offsetOf(date: string, zone: string): string {
  const guess = Date.parse(`${date}T12:00:00Z`);
  const parts = new Intl.DateTimeFormat("en-US", {
    timeZone: zone,
    timeZoneName: "longOffset",
  }).formatToParts(new Date(guess));
  const name = parts.find((part) => part.type === "timeZoneName")?.value ?? "GMT";
  const match = /GMT([+-]\d{2}:\d{2})/.exec(name);
  return match ? match[1] : "+00:00";
}

/** "Now" as an instant, given a civil date and a wall-clock time in the
 * DEVICE's zone — which every test below runs in (`TZ` is not pinned, so the
 * assertions are written to hold in any zone by using the same zone for
 * "today" that `deviceCivilToday` will read). */
function nowAt(date: string, hour = 9): number {
  return new Date(`${date}T${String(hour).padStart(2, "0")}:00:00`).getTime();
}

function readOf(events: CalendarEventDTO[], ageMs = 0): CalendarReadDTO {
  return { state: "read", events, freshness: { kind: "age", ageMs, declaredCadenceMs: null } };
}

function inputs(overrides: Partial<QuestionInputs> = {}): QuestionInputs {
  const bindings: BindingDTO[] = [
    { key: "trips-calendar", known: true, pending: false, value: { state: "text", text: "trips@g" } },
  ];
  return {
    bindings,
    paneReads: {},
    calendarReads: {},
    calendarConnected: true,
    items: [],
    nowMs: nowAt("2026-03-01"),
    ...overrides,
  };
}

describe("tripName", () => {
  it("strips a leading Trip:/Holiday: and nothing else", () => {
    expect(tripName("Trip: Lisbon")).toBe("Lisbon");
    expect(tripName("Holiday — India")).toBe("India");
    expect(tripName("Lisbon with the Dawsons")).toBe("Lisbon with the Dawsons");
  });

  it("never empties a title that is only the prefix", () => {
    expect(tripName("Trip:")).toBe("Trip:");
  });
});

describe("tripQueue — phases from civil dates", () => {
  const trip = allDay("t1", "Trip: Lisbon", "2026-03-10", "2026-03-16"); // 10th–15th

  it("counts to the first day in whole civil days", () => {
    const [next] = tripQueue([trip], "trips@g", nowAt("2026-03-01"));
    expect(next.phase).toBe("upcoming");
    expect(next.daysUntil).toBe(9);
    expect(next.lengthDays).toBe(6);
  });

  it("reads the exclusive end as the day AFTER the last day", () => {
    // `endMs - DAY` arithmetic would put the last day on the 16th here and
    // make the trip seven days long — ADR-0015's own recorded defect.
    const [next] = tripQueue([trip], "trips@g", nowAt("2026-03-15", 20));
    expect(next.phase).toBe("returns_today");
    expect(next.lengthDays).toBe(6);
    expect(next.dayOfTrip).toBe(6);
  });

  it("is still the trip on its return day, and gone the day after", () => {
    expect(tripQueue([trip], "trips@g", nowAt("2026-03-15")).length).toBe(1);
    expect(tripQueue([trip], "trips@g", nowAt("2026-03-16")).length).toBe(0);
  });

  it("departs today on the first day and is under way in between", () => {
    expect(tripQueue([trip], "trips@g", nowAt("2026-03-10"))[0].phase).toBe("departs_today");
    const midTrip = tripQueue([trip], "trips@g", nowAt("2026-03-12"))[0];
    expect(midTrip.phase).toBe("under_way");
    expect(midTrip.dayOfTrip).toBe(3);
  });

  it("counts whole days across a spring-forward DST boundary", () => {
    // Europe/Lisbon springs forward on 2026-03-29. A ms-subtraction day count
    // loses an hour there and rounds a 23-hour day away.
    const acrossDst = allDay("t2", "Trip: Porto", "2026-03-27", "2026-04-01");
    const [next] = tripQueue([acrossDst], "trips@g", nowAt("2026-03-25"));
    expect(next.daysUntil).toBe(2);
    expect(next.lengthDays).toBe(5);
  });

  it("resolves the return-day boundary in the event's zone, not the device's", () => {
    // A trip ending at local midnight in Kolkata: the reader's own day is
    // what decides "today", and the trip's last civil date is what the
    // event's zone says it is.
    const india = allDay("t3", "Trip: India", "2027-01-05", "2027-01-20", "Asia/Kolkata");
    const [next] = tripQueue([india], "trips@g", nowAt("2027-01-19"));
    expect(next.phase).toBe("returns_today");
    expect(next.lengthDays).toBe(15);
  });

  it("drops an event whose zone this runtime cannot read", () => {
    // `""` is a real value on the calendar wire (`protocol.ts`), and
    // `Intl.DateTimeFormat` throws on it. Guessing a zone would move the
    // whole trip; dropping it is `zoned-day.ts`'s rule.
    const broken = allDay("t4", "Trip: Nowhere", "2026-03-10", "2026-03-12", "Europe/Lisbon");
    broken.start = { ...broken.start, timeZone: "" };
    expect(tripQueue([broken], "trips@g", nowAt("2026-03-01"))).toEqual([]);
  });

  it("reads only the bound calendar, and never a cancelled instance", () => {
    const other = allDay("t5", "Standup", "2026-03-02", "2026-03-03", "Europe/Lisbon", "work@g");
    const cancelled = allDay("t6", "Trip: Oslo", "2026-03-04", "2026-03-06");
    cancelled.status = "cancelled";
    expect(tripQueue([other, cancelled, trip], "trips@g", nowAt("2026-03-01")).map((t) => t.id)).toEqual([
      "t1",
    ]);
  });

  it("orders the whole queue by first day, soonest first", () => {
    const later = allDay("t7", "Trip: Oslo", "2026-06-01", "2026-06-05");
    expect(
      tripQueue([later, trip], "trips@g", nowAt("2026-03-01")).map((t) => t.name),
    ).toEqual(["Lisbon", "Oslo"]);
  });

  it("takes a timed event on the trips calendar as a trip like any other", () => {
    // #121 §4: the calendar is the authority. A pane that decided some events
    // on the Trips calendar are not trips has started keeping a vacation
    // record of its own.
    const timed = allDay("t8", "Weekend in Sintra", "2026-04-03", "2026-04-06");
    timed.allDay = false;
    expect(tripQueue([timed], "trips@g", nowAt("2026-03-01")).length).toBe(1);
  });
});

describe("vacationBand", () => {
  const at = (start: string, endExclusive: string) => allDay("b", "Trip: Lisbon", start, endExclusive);

  it("is live for every day of the trip itself", () => {
    for (const today of ["2026-03-10", "2026-03-12", "2026-03-15"]) {
      const [trip] = tripQueue([at("2026-03-10", "2026-03-16")], "trips@g", nowAt(today));
      expect(vacationBand(trip)).toBe("live");
    }
  });

  it("climbs imminent -> near -> distant as the trip gets further away", () => {
    const ladder: [string, string][] = [
      ["2026-03-05", "imminent"],
      ["2026-02-20", "near"],
      ["2026-01-01", "distant"],
    ];
    for (const [today, band] of ladder) {
      const [trip] = tripQueue([at("2026-03-10", "2026-03-16")], "trips@g", nowAt(today));
      expect(vacationBand(trip)).toBe(band);
    }
  });

  it("keeps a trip 700 days out out of dormant", () => {
    // ADR-0015 names this pane as the reason "dormant is not a synonym for
    // far away": dormant means there is nothing to count to.
    const [trip] = tripQueue([at("2028-01-20", "2028-02-01")], "trips@g", nowAt("2026-03-01"));
    expect(vacationBand(trip)).toBe("distant");
  });

  it("is dormant only when nothing is booked", () => {
    expect(vacationBand(null)).toBe("dormant");
  });
});

describe("vacationHeadline", () => {
  const queue = (today: string, start = "2026-03-10", endExclusive = "2026-03-16") =>
    tripQueue([allDay("h", "Trip: Lisbon", start, endExclusive)], "trips@g", nowAt(today))[0];

  it("names the place first, then the count", () => {
    expect(vacationHeadline(queue("2026-02-22"))).toBe("Lisbon in 16 days");
  });

  it("says tomorrow and today rather than 1 day and 0 days", () => {
    expect(vacationHeadline(queue("2026-03-09"))).toBe("Lisbon tomorrow");
    expect(vacationHeadline(queue("2026-03-10"))).toBe("Lisbon today");
  });

  it("becomes a status line mid-trip and on the return day", () => {
    expect(vacationHeadline(queue("2026-03-12"))).toBe("In Lisbon · day 3 of 6");
    expect(vacationHeadline(queue("2026-03-15"))).toBe("Home today from Lisbon");
  });

  it("names the horizon when nothing is booked, never a bare 'nothing'", () => {
    expect(vacationHeadline(null)).toBe(`Nothing booked in the next ${HORIZON_LABEL}`);
  });
});

describe("isStaleFreshness", () => {
  it("never reads an unknown age as fresh", () => {
    expect(isStaleFreshness({ kind: "unknown" })).toBe(true);
  });

  it("turns over at 24 hours", () => {
    expect(isStaleFreshness({ kind: "age", ageMs: STALE_AFTER_MS - 1, declaredCadenceMs: null })).toBe(
      false,
    );
    expect(isStaleFreshness({ kind: "age", ageMs: STALE_AFTER_MS + 1, declaredCadenceMs: null })).toBe(
      true,
    );
  });
});

describe("vacationAnswer", () => {
  const trip = allDay("a1", "Trip: Lisbon", "2026-03-10", "2026-03-16");

  it("is unbound with no calendar connected at all, before anything else", () => {
    const answer = vacationAnswer(
      inputs({ calendarConnected: false, calendarReads: { [CALENDAR_REQUEST_KEY]: readOf([trip]) } }),
    );
    expect(answer.answerState).toBe("unbound");
    expect(answer.band).toBe("dormant");
    expect(answer.withinBand).toBeNull();
  });

  it("is unbound when no Trips calendar is designated", () => {
    for (const value of [
      { state: "unset" } as const,
      { state: "other", raw: "7" } as const,
      { state: "text", text: "  " } as const,
    ]) {
      const answer = vacationAnswer(
        inputs({
          bindings: [{ key: "trips-calendar", known: true, pending: false, value }],
          calendarReads: { [CALENDAR_REQUEST_KEY]: readOf([trip]) },
        }),
      );
      expect(answer.answerState).toBe("unbound");
    }
  });

  it("waits rather than claiming an answer while the read has not landed", () => {
    for (const read of [undefined, { state: "not_read" } as const]) {
      const answer = vacationAnswer(
        inputs({ calendarReads: read === undefined ? {} : { [CALENDAR_REQUEST_KEY]: read } }),
      );
      expect(answer.answerState).toBe("bound-but-unacquired");
      expect(answer.collapsedHeadline).toBe("Waiting for the first calendar sync");
    }
  });

  it("answers — not a gap — when the window holds no trip at all", () => {
    const answer = vacationAnswer(inputs({ calendarReads: { [CALENDAR_REQUEST_KEY]: readOf([]) } }));
    expect(answer.answerState).toBe("answered");
    expect(answer.band).toBe("dormant");
    expect(answer.withinBand).toBeNull();
    expect(answer.collapsedHeadline).toBe(`Nothing booked in the next ${HORIZON_LABEL}`);
  });

  it("sorts by the next relevant moment: the start while upcoming, the end while live", () => {
    const upcoming = vacationAnswer(
      inputs({ calendarReads: { [CALENDAR_REQUEST_KEY]: readOf([trip]) } }),
    );
    expect(upcoming.withinBand).toBe(trip.start.instantMs);

    const live = vacationAnswer(
      inputs({
        nowMs: nowAt("2026-03-12"),
        calendarReads: { [CALENDAR_REQUEST_KEY]: readOf([trip]) },
      }),
    );
    expect(live.withinBand).toBe(trip.end.instantMs);
  });

  it("carries no glyphs — one subject, and the answer is already a sentence", () => {
    const answer = vacationAnswer(inputs({ calendarReads: { [CALENDAR_REQUEST_KEY]: readOf([trip]) } }));
    expect(answer.icon).toBeUndefined();
  });

  it("still answers when the read is stale, and says so", () => {
    const view = vacationView(
      inputs({
        calendarReads: { [CALENDAR_REQUEST_KEY]: readOf([trip], STALE_AFTER_MS + 1) },
      }),
    );
    expect(view?.stale).toBe(true);
    expect(view?.next?.name).toBe("Lisbon");
    expect(vacationHeadline(view?.next ?? null)).toBe("Lisbon in 9 days");
  });

  it("lists the whole queue, never truncated", () => {
    const events = [
      trip,
      allDay("a2", "Trip: Oslo", "2026-06-01", "2026-06-05"),
      allDay("a3", "Trip: Tokyo", "2027-02-01", "2027-02-14"),
      allDay("a4", "Trip: India", "2027-04-01", "2027-04-20"),
    ];
    const view = vacationView(inputs({ calendarReads: { [CALENDAR_REQUEST_KEY]: readOf(events) } }));
    expect(view?.later.map((t) => t.name)).toEqual(["Oslo", "Tokyo", "India"]);
  });
});
