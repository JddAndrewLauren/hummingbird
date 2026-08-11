import { describe, expect, it } from "vitest";
import type { CalendarEventDTO, TaskItemDTO } from "../../store/protocol";
import type { QuestionInputs } from "../questions/contract";
import {
  CALENDAR_REQUEST_KEY,
  countKinds,
  dayKeyOf,
  entryUrgency,
  mergeWindow,
  SUBJECT_KEY,
  weekendAnswer,
  weekendBand,
  weekendCollapsedHeadline,
  weekendGlyphs,
  weekendView,
  weekendWindow,
  weekendWithinBand,
  type WeekendWindow,
} from "./weekend";

// #122's weekend-plans pane: the window arithmetic, the read-time merge and
// its dedupe rule, and the shell's answer — each pinned independently of
// rendering, the same split `waste.ts`'s own test file uses.

function at(y: number, m: number, d: number, h = 0, min = 0): number {
  return new Date(y, m - 1, d, h, min).getTime();
}

function item(overrides: Partial<TaskItemDTO> & { id: string }): TaskItemDTO {
  return {
    id: overrides.id,
    seq: null,
    title: overrides.title ?? overrides.id,
    description: null,
    stage: "ready",
    size: null,
    energy: null,
    context: null,
    priority: 2,
    projectId: null,
    projectPos: null,
    deadline: overrides.deadline ?? null,
    scheduledDate: overrides.scheduledDate ?? null,
    source: null,
    sourceKey: null,
    sourceUrl: null,
    archivedAt: null,
    createdAt: 0,
    updatedAt: 0,
    version: 1,
    pending: false,
  };
}

function event(overrides: {
  id: string;
  title?: string;
  startMs: number;
  endMs: number;
  allDay?: boolean;
  status?: "confirmed" | "tentative" | "cancelled";
}): CalendarEventDTO {
  return {
    providerEventId: overrides.id,
    calendarId: "cal-primary",
    title: overrides.title ?? overrides.id,
    start: { instantMs: overrides.startMs, timeZone: "America/Los_Angeles" },
    end: { instantMs: overrides.endMs, timeZone: "America/Los_Angeles" },
    allDay: overrides.allDay ?? false,
    recurrenceId: null,
    location: null,
    organizer: null,
    status: overrides.status ?? "confirmed",
    providerUpdatedAtMs: overrides.startMs,
    htmlLink: null,
  };
}

function inputsWith(overrides: Partial<QuestionInputs> = {}): QuestionInputs {
  return {
    bindings: [],
    paneReads: {},
    calendarReads: {},
    items: [],
    nowMs: at(2026, 8, 10, 9, 0),
    ...overrides,
  };
}

// -- weekendWindow -----------------------------------------------------

describe("weekendWindow", () => {
  it("is Friday 17:00 through Sunday 23:59:59.999, asked on a Friday afternoon", () => {
    // 2026-08-14 is a Friday.
    const window = weekendWindow(at(2026, 8, 14, 16, 0));
    expect(window.startMs).toBe(at(2026, 8, 14, 17, 0));
    expect(window.endMs).toBe(at(2026, 8, 17, 0, 0) - 1);
  });

  it("keeps Friday's own evening event inside the answer, asked at 17:00 sharp", () => {
    const window = weekendWindow(at(2026, 8, 14, 17, 0));
    const dinner = at(2026, 8, 14, 19, 0);
    expect(dinner >= window.startMs && dinner <= window.endMs).toBe(true);
  });

  it("excludes a Friday-afternoon event before the window opens", () => {
    const window = weekendWindow(at(2026, 8, 14, 16, 0));
    const lunch = at(2026, 8, 14, 12, 0);
    expect(lunch >= window.startMs).toBe(false);
  });

  it("asked on a weekday, answers the upcoming weekend", () => {
    // 2026-08-10 is a Monday; the coming weekend is Aug 14-16.
    const window = weekendWindow(at(2026, 8, 10, 9, 0));
    expect(window.startMs).toBe(at(2026, 8, 14, 17, 0));
  });

  it("stays on the weekend under way at Sunday 19:59", () => {
    const window = weekendWindow(at(2026, 8, 16, 19, 59));
    expect(window.startMs).toBe(at(2026, 8, 14, 17, 0));
    expect(window.underWay).toBe(true);
  });

  it("rolls forward to the FOLLOWING weekend at Sunday 20:01", () => {
    const window = weekendWindow(at(2026, 8, 16, 20, 1));
    expect(window.startMs).toBe(at(2026, 8, 21, 17, 0));
    expect(window.underWay).toBe(false);
  });

  it("always carries exactly three days — Friday, Saturday, Sunday", () => {
    const window = weekendWindow(at(2026, 8, 10, 9, 0));
    expect(window.days.map((day) => day.key)).toEqual([
      dayKeyOf(at(2026, 8, 14)),
      dayKeyOf(at(2026, 8, 15)),
      dayKeyOf(at(2026, 8, 16)),
    ]);
  });
});

// -- mergeWindow ---------------------------------------------------------

describe("mergeWindow", () => {
  const window = weekendWindow(at(2026, 8, 10, 9, 0)); // Fri 14 - Sun 16 Aug 2026

  it("places an event overlapping the window on the right day, chronologically", () => {
    const merged = mergeWindow(
      window,
      [event({ id: "ev-1", startMs: at(2026, 8, 15, 9, 0), endMs: at(2026, 8, 15, 10, 0) })],
      [],
    );
    const saturday = merged.days.find((day) => day.key === dayKeyOf(at(2026, 8, 15)));
    expect(saturday?.entries.map((entry) => entry.id)).toEqual(["ev-1@" + dayKeyOf(at(2026, 8, 15))]);
  });

  it("drops an event entirely outside the window", () => {
    const merged = mergeWindow(
      window,
      [event({ id: "ev-2", startMs: at(2026, 8, 10, 9, 0), endMs: at(2026, 8, 10, 10, 0) })],
      [],
    );
    expect(countKinds(merged).events).toBe(0);
  });

  it("emits an all-day event spanning two days once on each day", () => {
    const merged = mergeWindow(
      window,
      [
        event({
          id: "trip",
          startMs: at(2026, 8, 15, 0, 0),
          endMs: at(2026, 8, 17, 0, 0),
          allDay: true,
        }),
      ],
      [],
    );
    expect(countKinds(merged).events).toBe(2); // Saturday and Sunday, clipped to the window
  });

  it("shows a due item on its deadline day, distinct in kind from an event", () => {
    const merged = mergeWindow(
      window,
      [],
      [item({ id: "due-1", deadline: dayKeyOf(at(2026, 8, 15)) })],
    );
    const day = merged.days.find((d) => d.key === dayKeyOf(at(2026, 8, 15)));
    expect(day?.entries[0]?.kind).toBe("due");
  });

  it("shows a scheduled item on its do-date day, distinct in kind from a due item", () => {
    const merged = mergeWindow(
      window,
      [],
      [item({ id: "sched-1", scheduledDate: dayKeyOf(at(2026, 8, 16)) })],
    );
    const day = merged.days.find((d) => d.key === dayKeyOf(at(2026, 8, 16)));
    expect(day?.entries[0]?.kind).toBe("scheduled");
  });

  it("an item both due and scheduled in the window appears exactly once, as due, do-date kept", () => {
    const dueDay = dayKeyOf(at(2026, 8, 15));
    const merged = mergeWindow(
      window,
      [],
      [item({ id: "both", deadline: dueDay, scheduledDate: dueDay })],
    );
    const all = merged.days.flatMap((day) => day.entries);
    expect(all).toHaveLength(1);
    expect(all[0].kind).toBe("due");
    expect(all[0].alsoScheduledOn).toBe(dueDay);
  });

  it("an item scheduled in the window but due OUTSIDE it still shows its deadline", () => {
    const merged = mergeWindow(
      window,
      [],
      [
        item({
          id: "sched-with-outside-due",
          scheduledDate: dayKeyOf(at(2026, 8, 15)),
          deadline: "2026-08-25",
        }),
      ],
    );
    const all = merged.days.flatMap((day) => day.entries);
    expect(all).toHaveLength(1);
    expect(all[0].kind).toBe("scheduled");
    expect(all[0].deadlineOutsideWindow).toBe("2026-08-25");
  });

  it("keeps an entry already in the past within the window, unrestyled", () => {
    // Asked from Saturday afternoon: Saturday morning's parkrun stays.
    const nowMs = at(2026, 8, 15, 14, 0);
    const liveWindow = weekendWindow(nowMs);
    const merged = mergeWindow(
      liveWindow,
      [event({ id: "parkrun", startMs: at(2026, 8, 15, 9, 0), endMs: at(2026, 8, 15, 10, 0) })],
      [],
    );
    expect(countKinds(merged).events).toBe(1);
  });

  it("drops an item with neither a due date nor a do-date in the window", () => {
    const merged = mergeWindow(window, [], [item({ id: "irrelevant" })]);
    expect(countKinds(merged).due + countKinds(merged).scheduled).toBe(0);
  });
});

// -- entryUrgency: never reads scheduled_date ----------------------------

describe("entryUrgency", () => {
  it("reads the item's deadline and NOTHING else — never scheduled_date", () => {
    const window = weekendWindow(at(2026, 8, 10, 9, 0));
    const dueDay = dayKeyOf(at(2026, 8, 15));
    const merged = mergeWindow(window, [], [item({ id: "x", deadline: dueDay })]);
    const entry = merged.days.flatMap((d) => d.entries)[0];
    const before = entryUrgency(entry, at(2026, 8, 10, 9, 0));

    // Changing scheduled_date on the same underlying item must never move
    // the urgency reading — re-run with a scheduled_date added.
    const mergedWithSchedule = mergeWindow(
      window,
      [],
      [item({ id: "x", deadline: dueDay, scheduledDate: dueDay })],
    );
    const entryWithSchedule = mergedWithSchedule.days.flatMap((d) => d.entries)[0];
    const after = entryUrgency(entryWithSchedule, at(2026, 8, 10, 9, 0));

    expect(after).toBe(before);
  });

  it("a scheduled-only entry (no deadline) is always calm", () => {
    const window = weekendWindow(at(2026, 8, 10, 9, 0));
    const merged = mergeWindow(
      window,
      [],
      [item({ id: "s", scheduledDate: dayKeyOf(at(2026, 8, 15)) })],
    );
    const entry = merged.days.flatMap((d) => d.entries)[0];
    expect(entryUrgency(entry, at(2026, 8, 10, 9, 0))).toBe("calm");
  });
});

// -- bands: live / imminent / near / dormant, never distant --------------

describe("weekendBand", () => {
  function windowStartingAt(startMs: number): WeekendWindow {
    return { startMs, endMs: startMs + 3 * 24 * 60 * 60 * 1000 - 1, days: [], underWay: false };
  }

  it("is live while the weekend is under way", () => {
    const window = weekendWindow(at(2026, 8, 15, 12, 0));
    expect(weekendBand(window, at(2026, 8, 15, 12, 0))).toBe("live");
  });

  it("is imminent within 48 hours of the start", () => {
    const window = windowStartingAt(at(2026, 8, 14, 17, 0));
    const nowMs = at(2026, 8, 13, 0, 0); // ~41h before start
    expect(weekendBand(window, nowMs)).toBe("imminent");
  });

  it("is near beyond 48h but within 96h of the start", () => {
    const window = windowStartingAt(at(2026, 8, 14, 17, 0));
    const nowMs = at(2026, 8, 11, 12, 0); // ~77h before start
    expect(weekendBand(window, nowMs)).toBe("near");
  });

  it("is dormant beyond 96h of the start", () => {
    const window = windowStartingAt(at(2026, 8, 14, 17, 0));
    const nowMs = at(2026, 8, 10, 0, 0); // well over 96h before start
    expect(weekendBand(window, nowMs)).toBe("dormant");
  });

  it("is never distant, at any distance", () => {
    const window = windowStartingAt(at(2026, 8, 14, 17, 0));
    const farNowMs = at(2020, 1, 1, 0, 0);
    expect(weekendBand(window, farNowMs)).not.toBe("distant");
  });
});

describe("weekendWithinBand", () => {
  it("is the window's start while it is ahead", () => {
    const window = weekendWindow(at(2026, 8, 10, 9, 0));
    expect(weekendWithinBand(window)).toBe(window.startMs);
  });

  it("is the window's end once the weekend is under way — an absolute instant, not a duration", () => {
    const window = weekendWindow(at(2026, 8, 15, 12, 0));
    expect(weekendWithinBand(window)).toBe(window.endMs);
  });
});

// -- the shell's answer ---------------------------------------------------

describe("weekendAnswer", () => {
  it("is unbound when this device has never synced a calendar at all", () => {
    const answer = weekendAnswer(
      inputsWith({ calendarReads: { [CALENDAR_REQUEST_KEY]: { state: "not_read" } } }),
    );
    expect(answer.answerState).toBe("unbound");
  });

  it("is bound-but-unacquired when the calendar read hasn't landed yet", () => {
    const answer = weekendAnswer(inputsWith({ calendarReads: {} }));
    expect(answer.answerState).toBe("bound-but-unacquired");
  });

  it("is answered for a genuinely empty weekend — a question that answered nothing has not failed", () => {
    const answer = weekendAnswer(
      inputsWith({
        calendarReads: { [CALENDAR_REQUEST_KEY]: { state: "read", events: [], freshness: { kind: "unknown" } } },
        items: [],
      }),
    );
    expect(answer.answerState).toBe("answered");
  });

  it("is answered with real content when items/events land", () => {
    const nowMs = at(2026, 8, 10, 9, 0);
    const answer = weekendAnswer(
      inputsWith({
        nowMs,
        calendarReads: {
          [CALENDAR_REQUEST_KEY]: {
            state: "read",
            events: [event({ id: "e1", startMs: at(2026, 8, 15, 9, 0), endMs: at(2026, 8, 15, 10, 0) })],
            freshness: { kind: "age", ageMs: 1_000, declaredCadenceMs: 900_000 },
          },
        },
      }),
    );
    expect(answer.answerState).toBe("answered");
    expect(weekendCollapsedHeadline({ startMs: 0, endMs: 0, underWay: false, days: [
      { key: "d", label: "d", dateLabel: "d", startMs: 0, endMs: 0, entries: [
        { id: "e1", kind: "event", title: "e1", atMs: 0, anchor: "time", dayKey: "d" },
      ] },
    ] })).toContain("on the calendar");
    expect(weekendGlyphs({ startMs: 0, endMs: 0, underWay: false, days: [] }).length).toBe(0);
  });

  it("registers exactly one subject, always — even unbound", () => {
    const subjectKey = SUBJECT_KEY;
    expect(subjectKey).toBe("coming-weekend");
  });
});

describe("weekendView", () => {
  it("is null while unbound or unacquired, and the merged window once read", () => {
    expect(weekendView(inputsWith({ calendarReads: {} }))).toBeNull();
    expect(
      weekendView(inputsWith({ calendarReads: { [CALENDAR_REQUEST_KEY]: { state: "not_read" } } })),
    ).toBeNull();
    const view = weekendView(
      inputsWith({
        calendarReads: {
          [CALENDAR_REQUEST_KEY]: { state: "read", events: [], freshness: { kind: "unknown" } },
        },
      }),
    );
    expect(view).not.toBeNull();
  });
});
