import { describe, expect, it } from "vitest";
import type { CalendarEventDTO, CalendarReadDTO } from "../../store/protocol";
import { EMPTY_QUESTION_SYNC, type QuestionInputs } from "../questions/contract";
import {
  CALENDAR_REQUEST_KEY,
  HORIZON_AFTER_DAYS,
  monthName,
  scpsAnswer,
  scpsCalendarInterval,
  scpsCardTitle,
  scpsCollapsedHeadline,
  scpsHeadline,
  scpsQuestLine,
  scpsTimeLabel,
  scpsView,
} from "./scps";

function timed(
  id: string,
  title: string,
  startAt: string,
  endAt: string,
  calendarId = "primary",
): CalendarEventDTO {
  const at = (stamp: string) => new Date(`${stamp}:00`).getTime();
  return {
    providerEventId: id,
    calendarId,
    title,
    when: { kind: "timed", startMs: at(startAt), endMs: at(endAt) },
    recurrenceId: null,
    location: null,
    organizer: null,
    status: "confirmed",
    providerUpdatedAtMs: 0,
    htmlLink: null,
    description: null,
  };
}

function nowAt(date: string, hour = 9): number {
  return new Date(`${date}T${String(hour).padStart(2, "0")}:00:00`).getTime();
}

function readOf(events: CalendarEventDTO[], ageMs = 0): CalendarReadDTO {
  return { state: "read", events, freshness: { kind: "age", ageMs, declaredCadenceMs: null } };
}

function inputs(overrides: Partial<QuestionInputs> = {}): QuestionInputs {
  return {
    sync: EMPTY_QUESTION_SYNC,
    bindings: [],
    paneReads: {},
    calendarReads: {},
    calendarConnected: true,
    items: [],
    nowMs: nowAt("2026-03-01"),
    ...overrides,
  };
}

function withEvents(events: CalendarEventDTO[], overrides: Partial<QuestionInputs> = {}): QuestionInputs {
  return inputs({ calendarReads: { [CALENDAR_REQUEST_KEY]: readOf(events) }, ...overrides });
}

describe("scpsCalendarInterval", () => {
  it("is the standard horizon: 6 hours behind now, 90 days ahead", () => {
    const nowMs = nowAt("2026-03-01", 12);
    const interval = scpsCalendarInterval(nowMs);
    expect(interval.startMs).toBe(nowMs - 6 * 60 * 60 * 1000);
    expect(interval.endMs).toBe(nowMs + HORIZON_AFTER_DAYS * 24 * 60 * 60 * 1000);
    expect(interval.endDate > interval.startDate).toBe(true);
  });
});

describe("scpsView / scpsAnswer", () => {
  it("waits rather than claiming an answer before the first calendar sync", () => {
    const stateInputs = inputs({ calendarConnected: false });
    expect(scpsView(stateInputs)).toBeNull();
    expect(scpsAnswer(stateInputs).answerState).toBe("bound-but-unacquired");
  });

  it("answers dormant, never unbound, when the window holds no SCPS event", () => {
    const answer = scpsAnswer(withEvents([]));
    expect(answer.answerState).toBe("answered");
    expect(answer.band).toBe("dormant");
    expect(answer.collapsedHeadline).toBe("No SCPS event scheduled");
  });

  it("ignores a non-SCPS event and a cancelled SCPS one", () => {
    const nonScps = timed("e1", "Team Sync", "2026-03-05T09:00", "2026-03-05T10:00");
    const cancelled: CalendarEventDTO = {
      ...timed("e2", "SCPS Meeting: X", "2026-03-05T09:00", "2026-03-05T10:00"),
      status: "cancelled",
    };
    const view = scpsView(withEvents([nonScps, cancelled]));
    expect(view?.next ?? null).toBeNull();
  });

  it("bands live for an event in progress or starting today, climbing through the ladder further out", () => {
    const today = timed("e1", "SCPS Meeting: Today", "2026-03-01T14:00", "2026-03-01T16:00");
    expect(scpsAnswer(withEvents([today])).band).toBe("live");

    const tomorrow = timed("e2", "SCPS Activity: Tide Pools", "2026-03-02T09:00", "2026-03-02T12:00");
    expect(scpsAnswer(withEvents([tomorrow])).band).toBe("imminent");

    const near = timed("e3", "SCPS Meeting: X", "2026-03-06T09:00", "2026-03-06T10:00");
    expect(scpsAnswer(withEvents([near])).band).toBe("near");

    const distant = timed("e4", "SCPS Meeting: X", "2026-03-20T09:00", "2026-03-20T10:00");
    expect(scpsAnswer(withEvents([distant])).band).toBe("distant");
  });

  it("lists further SCPS events in the window, never truncated", () => {
    const events = [
      timed("e1", "SCPS Meeting: A", "2026-03-05T09:00", "2026-03-05T10:00"),
      timed("e2", "SCPS Activity: B", "2026-03-10T09:00", "2026-03-10T12:00"),
      timed("e3", "SCPS Happy Hour", "2026-03-15T17:00", "2026-03-15T18:00"),
    ];
    const view = scpsView(withEvents(events));
    expect(view?.next?.id).toBe("e1");
    expect(view?.later.map((e) => e.id)).toEqual(["e2", "e3"]);
  });
});

describe("scpsHeadline", () => {
  it("composes the kind, the day, the time and the topic", () => {
    const event = timed("e1", "SCPS Activity: Super Girl Surf Festival", "2026-03-07T09:00", "2026-03-07T12:00");
    const view = scpsView(withEvents([event], { nowMs: nowAt("2026-03-01") }));
    const headline = scpsHeadline(view?.next ?? null);
    expect(headline).toMatch(/^SCPS Activity /);
    expect(headline).toContain("9:00am");
    expect(headline).toContain("— Super Girl Surf Festival");
  });

  it("says today for an event starting today", () => {
    const event = timed("e1", "SCPS Meeting: Impressions of Venice", "2026-03-01T14:00", "2026-03-01T16:00");
    const view = scpsView(withEvents([event], { nowMs: nowAt("2026-03-01", 9) }));
    expect(scpsHeadline(view?.next ?? null)).toBe("SCPS Meeting today 2:00pm — Impressions of Venice");
  });

  it("says a bare happy hour in progress with no time or topic", () => {
    const event = timed("e1", "SCPS Happy Hour", "2026-03-01T17:00", "2026-03-01T19:00");
    const view = scpsView(withEvents([event], { nowMs: nowAt("2026-03-01", 18) }));
    expect(scpsHeadline(view?.next ?? null)).toBe("SCPS Happy Hour in progress");
  });

  it("reads dormant as 'No SCPS event scheduled'", () => {
    expect(scpsHeadline(null)).toBe("No SCPS event scheduled");
  });
});

describe("scpsCardTitle", () => {
  function nextOf(event: CalendarEventDTO, nowMs: number) {
    const next = scpsView(withEvents([event], { nowMs }))?.next;
    if (next == null) {
      throw new Error("fixture produced no next event");
    }
    return next;
  }

  it("titles with kind and topic only — the day and time belong to the meta line", () => {
    const event = timed("e1", "SCPS Meeting: Impressions of Venice", "2026-03-01T14:00", "2026-03-01T16:00");
    expect(scpsCardTitle(nextOf(event, nowAt("2026-03-01", 9)))).toBe(
      "SCPS Meeting — Impressions of Venice",
    );
  });

  it("keeps the in-progress happy hour sentence, and drops an absent topic's dash", () => {
    const event = timed("e1", "SCPS Happy Hour", "2026-03-01T17:00", "2026-03-01T19:00");
    expect(scpsCardTitle(nextOf(event, nowAt("2026-03-01", 18)))).toBe("SCPS Happy Hour in progress");
    expect(scpsCardTitle(nextOf(event, nowAt("2026-03-01", 9)))).toBe("SCPS Happy Hour");
  });
});

describe("scpsDayLabel / scpsTimeLabel", () => {
  it("formats 2:00pm and 9:00am plainly", () => {
    expect(scpsTimeLabel(nowAt("2026-03-01", 14))).toBe("2:00pm");
    expect(scpsTimeLabel(nowAt("2026-03-01", 9))).toBe("9:00am");
  });
});

describe("monthName", () => {
  it("reads the two-digit month token", () => {
    expect(monthName("2026-09")).toBe("September");
    expect(monthName("2026-01")).toBe("January");
  });
});

describe("scpsQuestLine / scpsCollapsedHeadline", () => {
  const questBinding = (value: string) => [
    { key: "scps-quest", known: true, pending: false, value: { state: "text" as const, text: value } },
  ];

  it("shows the current month's phrase", () => {
    const withQuest = withEvents([], {
      nowMs: nowAt("2026-09-15"),
      bindings: questBinding("2026-09 Reflected Light"),
    });
    const view = scpsView(withQuest);
    expect(view?.quest).toEqual({ kind: "current", phrase: "Reflected Light" });
    expect(scpsQuestLine(view!.quest, withQuest.nowMs)).toBe("Photo Quest — Reflected Light");
    expect(scpsCollapsedHeadline(null, view!.quest)).toBe("No SCPS event scheduled · Quest: Reflected Light");
  });

  it("names the posted month for a last month's quest, and appends nothing to the collapsed row", () => {
    const stale = withEvents([], {
      nowMs: nowAt("2026-10-01"),
      bindings: questBinding("2026-09 Reflected Light"),
    });
    const view = scpsView(stale);
    expect(view?.quest).toEqual({ kind: "other", month: "2026-09", phrase: "Reflected Light" });
    expect(scpsQuestLine(view!.quest, stale.nowMs)).toBe(
      "No quest posted for October; last: Reflected Light (September)",
    );
    expect(scpsCollapsedHeadline(null, view!.quest)).toBe("No SCPS event scheduled");
  });

  it("reads unset or unparseable quest values as 'No quest set'", () => {
    for (const value of ["reflections", "2026-9 x", ""]) {
      const unset = withEvents([], { bindings: questBinding(value) });
      const view = scpsView(unset);
      expect(view?.quest).toEqual({ kind: "none" });
      expect(scpsQuestLine(view!.quest, unset.nowMs)).toBe("No quest set");
    }
  });
});
