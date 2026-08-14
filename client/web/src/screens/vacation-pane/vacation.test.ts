import { describe, expect, it } from "vitest";
import type { BindingDTO, CalendarEventDTO, CalendarReadDTO } from "../../store/protocol";
import { EMPTY_QUESTION_SYNC, type QuestionInputs } from "../questions/contract";
import { addCivilDays, deviceCivilToday } from "../waste-pane/zoned-day";
import {
  CALENDAR_REQUEST_KEY,
  HORIZON_AHEAD_DAYS,
  HORIZON_LABEL,
  STALE_AFTER_MS,
  isStaleFreshness,
  tripName,
  tripQueue,
  vacationAnswer,
  vacationBand,
  vacationCalendarInterval,
  vacationHeadline,
  vacationView,
} from "./vacation";

// Every all-day date below is the provider's civil date directly — the pane's
// whole contract is that it never flattens one to an instant for day counts.

/** An all-day event, the provider's own civil dates. `endDate` is exclusive. */
function allDay(
  id: string,
  title: string,
  startDate: string,
  endExclusiveDate: string,
  calendarId = "trips@g",
): CalendarEventDTO {
  return {
    providerEventId: id,
    calendarId,
    title,
    when: { kind: "allDay", startDate, endDate: endExclusiveDate },
    recurrenceId: null,
    location: null,
    organizer: null,
    status: "confirmed",
    providerUpdatedAtMs: 0,
    htmlLink: null,
  };
}

/** A timed event, boundaries given as device-local wall-clock stamps
 * (`YYYY-MM-DDTHH:MM`). Its `endMs` is the real end instant. */
function timed(
  id: string,
  title: string,
  startAt: string,
  endAt: string,
  calendarId = "trips@g",
): CalendarEventDTO {
  const at = (stamp: string) => new Date(`${stamp}:00`).getTime();
  return {
    ...allDay(id, title, "2026-01-01", "2026-01-02", calendarId),
    when: { kind: "timed", startMs: at(startAt), endMs: at(endAt) },
  };
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
    sync: EMPTY_QUESTION_SYNC,
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

describe("vacationCalendarInterval", () => {
  it("includes an all-day trip on the +730d horizon day with an exclusive civil end", () => {
    const interval = vacationCalendarInterval(nowAt("2026-03-01", 12));
    const horizonDay = deviceCivilToday(interval.endMs);
    expect(horizonDay).not.toBeNull();
    const dayAfterHorizon = addCivilDays(horizonDay!, 1);

    // The instant arm reaches noon-ish on the horizon day (DST may move the
    // wall clock by an hour), so timed events earlier that day are included.
    const timedOnHorizonDay = new Date(`${horizonDay}T00:30:00`).getTime();
    expect(timedOnHorizonDay).toBeLessThan(interval.endMs);

    // The civil arm represents that same final day as a whole-day fact. Its
    // end is exclusive, so the day itself must be strictly below `endDate`.
    expect(interval.endDate).toBe(dayAfterHorizon);
    expect(horizonDay! < interval.endDate).toBe(true);
    const horizonTrip = allDay("horizon", "Trip: Horizon", horizonDay!, dayAfterHorizon!);
    expect(horizonTrip.when).toEqual({
      kind: "allDay",
      startDate: horizonDay,
      endDate: dayAfterHorizon,
    });
    if (horizonTrip.when.kind === "allDay") {
      expect(
        horizonTrip.when.startDate < interval.endDate &&
          horizonTrip.when.endDate > interval.startDate,
      ).toBe(true);
    }
    expect(HORIZON_AHEAD_DAYS).toBe(730);
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

  it("keeps the provider's all-day dates without resolving them through a zone", () => {
    // The provider already said which civil days India occupies. Flattening
    // these to instants and reading them back in the device zone caused the
    // original "India in 394 days" defect.
    const india = allDay("t3", "Trip: India", "2027-01-05", "2027-01-20");
    const [next] = tripQueue([india], "trips@g", nowAt("2027-01-19"));
    expect(next.phase).toBe("returns_today");
    expect(next.lengthDays).toBe(15);
  });

  it("drops an event whose provider civil dates are malformed", () => {
    const broken = allDay("t4", "Trip: Nowhere", "not-a-date", "2026-03-12");
    expect(tripQueue([broken], "trips@g", nowAt("2026-03-01"))).toEqual([]);
  });

  it("reads only the bound calendar, and never a cancelled instance", () => {
    const other = allDay("t5", "Standup", "2026-03-02", "2026-03-03", "work@g");
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

  it("reads a timed event's end as its real end, not an exclusive one", () => {
    // #121 §4: the calendar is the authority, all-day or timed. The all-day
    // "minus one civil day" rule applied here would end this trip on the 5th —
    // a short `lengthDays` and `returns_today` a day early.
    const trip = timed("t8", "Weekend in Sintra", "2026-04-03T18:30", "2026-04-06T21:00");
    const [next] = tripQueue([trip], "trips@g", nowAt("2026-03-01"));
    expect(next.lastDate).toBe("2026-04-06");
    expect(next.lengthDays).toBe(4);
    expect(tripQueue([trip], "trips@g", nowAt("2026-04-06"))[0].phase).toBe("returns_today");
    expect(tripQueue([trip], "trips@g", nowAt("2026-04-07")).length).toBe(0);
  });

  it("keeps a same-day timed event in the queue", () => {
    // The same misapplied rule puts `lastDate` BEFORE `startDate` here, which
    // reads as `past` and drops the event out of the queue with no trace.
    const day = timed("t9", "Trip: Porto day out", "2026-04-03T07:15", "2026-04-03T22:40");
    const [next] = tripQueue([day], "trips@g", nowAt("2026-04-03"));
    expect(next.phase).toBe("departs_today");
    expect(next.lengthDays).toBe(1);
    expect(tripQueue([day], "trips@g", nowAt("2026-04-01"))[0].daysUntil).toBe(2);
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
    expect(upcoming.withinBand).toBe(new Date("2026-03-10T00:00:00").getTime());

    const live = vacationAnswer(
      inputs({
        nowMs: nowAt("2026-03-12"),
        calendarReads: { [CALENDAR_REQUEST_KEY]: readOf([trip]) },
      }),
    );
    expect(live.withinBand).toBe(new Date("2026-03-16T00:00:00").getTime());
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
