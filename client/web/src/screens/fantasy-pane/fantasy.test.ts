import { describe, expect, it } from "vitest";
import type { BindingDTO } from "../../store/protocol";
import { EMPTY_QUESTION_SYNC, type QuestionInputs } from "../questions/contract";
import { rankPanes } from "../questions/registry";
import {
  BINDING_KEY,
  LINEUP_SOURCE,
  fantasyAnswer,
  fantasyGapReason,
  fantasySetup,
  fantasySubjects,
  leagueLabel,
  leaguesFromBinding,
  lineupHeadline,
  lockTime,
  problemWords,
  shortDuration,
} from "./fantasy";
import { GOLDEN_INVALID, GOLDEN_VALID, HOUR, LINEUP_SUBJECT, THURSDAY, lineupWorld } from "./golden-lineup.testkit";

// The decisions are `fantasy.rs`'s and are tested there; these pin that the
// web reads them through the seam and puts the right words on them.

function inputs(overrides: Partial<QuestionInputs> = {}): QuestionInputs {
  return {
    sync: EMPTY_QUESTION_SYNC,
    bindings: [],
    paneReads: {},
    calendarReads: {},
    calendarConnected: false,
    items: [],
    nowMs: 0,
    ...overrides,
  };
}

function binding(value: BindingDTO["value"]): BindingDTO {
  return { key: BINDING_KEY, known: true, pending: false, value };
}

function leagues(text: string): QuestionInputs {
  return inputs({ bindings: [binding({ state: "text", text })] });
}

describe("the league list", () => {
  it("is the core's parse: trimmed, empties skipped", () => {
    expect(leaguesFromBinding(" 449.l.1 ,449.l.2,")).toEqual(["449.l.1", "449.l.2"]);
  });

  it("reads a blanked row as unset and a non-text row as unusable", () => {
    expect(fantasySetup(leagues("  , ")).kind).toBe("unset");
    expect(fantasySetup(inputs({ bindings: [binding({ state: "other", raw: "[\"449.l.1\"]" })] })).kind).toBe(
      "unusable",
    );
  });
});

describe("fantasyAnswer", () => {
  it("unbound: both setup panes, dormant, with the setup prompt", () => {
    const world = inputs();
    expect(fantasySubjects(world)).toEqual(["setup:lineup", "setup:waivers"]);
    const [lineup, waivers] = fantasySubjects(world).map((subject) => fantasyAnswer(subject, world));
    expect(lineup.answerState).toBe("unbound");
    expect(lineup.band).toBe("dormant");
    expect(lineup.collapsedHeadline).toBe("Lineup · Not set up");
    expect(waivers.collapsedHeadline).toBe("Waivers · Not set up");
  });

  it("one league: two never-polled panes", () => {
    const world = leagues("449.l.123456");
    const answers = fantasySubjects(world).map((subject) => fantasyAnswer(subject, world));
    expect(answers.map((answer) => answer.collapsedHeadline)).toEqual([
      "League 123456 · Lineup · Never polled",
      "League 123456 · Waivers · Never polled",
    ]);
    expect(answers.every((answer) => answer.answerState === "bound-but-unacquired")).toBe(true);
    expect(answers.every((answer) => answer.band === "dormant")).toBe(true);
    expect(fantasyGapReason("449.l.123456:lineup", world)).toBe(
      "This league's lineup has not been polled yet.",
    );
  });

  it("a lineup row that is not a lineup is a gap, never an answer", () => {
    const world = inputs({
      bindings: [binding({ state: "text", text: "449.l.1" })],
      paneReads: {
        [LINEUP_SOURCE]: {
          source: LINEUP_SOURCE,
          snapshots: [
            {
              key: "449.l.1",
              fetchedAtMs: 0,
              envelope: { kind: "ok", schema: LINEUP_SOURCE, polledEveryMs: 21_600_000, body: "{}" },
              freshness: { kind: "age", ageMs: 60_000, declaredCadenceMs: 21_600_000 },
            },
          ],
          liveAlerts: [],
        },
      },
    });
    expect(fantasyAnswer("449.l.1:lineup", world).answerState).toBe("bound-but-unacquired");
    expect(fantasyGapReason("449.l.1:lineup", world)).toBe("The lineup payload isn't one this app can read.");
  });

  it("names a league by its id, and leaves an odd key whole", () => {
    expect(leagueLabel("449.l.123456")).toBe("League 123456");
    expect(leagueLabel("weird")).toBe("weird");
  });
});

describe("the Now surface", () => {
  it("ranks two fantasy panes per league — four for two leagues — and none when switched off", () => {
    const two = leagues("449.l.1,449.l.2");
    expect(rankPanes(two, "now").filter((pane) => pane.question === "fantasy")).toHaveLength(4);
    const off = { ...two, disabledQuestions: ["fantasy" as const] };
    expect(rankPanes(off, "now").some((pane) => pane.question === "fantasy")).toBe(false);
  });
});

// #855: the lineup's real answer, over the poller's own golden bodies. The
// band and the headline's decision are the core's (`fantasy.rs`); these pin
// that the web reads them and words them.

/** The invalid golden with only the slots whose reason is `keep` left
 * invalid. */
function onlyInvalid(keep: string): unknown {
  const body = structuredClone(GOLDEN_INVALID) as { slots: { invalid: { reason: string } | null }[] };
  for (const slot of body.slots) {
    if (slot.invalid !== null && slot.invalid.reason !== keep) {
      slot.invalid = null;
    }
  }
  return body;
}

describe("the lineup's answer", () => {
  it("valid: dormant, and the headline names when it locks", () => {
    const world = lineupWorld(GOLDEN_VALID, THURSDAY - 2 * 24 * HOUR);
    const answer = fantasyAnswer(LINEUP_SUBJECT, world);
    expect(answer.answerState).toBe("answered");
    expect(answer.band).toBe("dormant");
    expect(answer.collapsedHeadline).toBe(
      `Twinion Family League · Lineup set — locks ${lockTime(THURSDAY, world.nowMs)}`,
    );
  });

  it("invalid more than a day out: near, counted, with the time to lock", () => {
    const world = lineupWorld(GOLDEN_INVALID, THURSDAY - 30 * HOUR);
    const answer = fantasyAnswer(LINEUP_SUBJECT, world);
    expect(answer.band).toBe("near");
    expect(answer.collapsedHeadline).toBe("Twinion Family League · 3 slots to fix — locks in 30h");
  });

  it("invalid within a day: imminent", () => {
    const imminent = fantasyAnswer(LINEUP_SUBJECT, lineupWorld(GOLDEN_INVALID, THURSDAY - 4 * HOUR));
    expect(imminent.band).toBe("imminent");
    expect(imminent.collapsedHeadline).toBe("Twinion Family League · 3 slots to fix — locks in 4h");
  });

  it("one shared reason is named, and the lock is that slot's own deadline", () => {
    // Only the empty RB left invalid: its deadline is Sunday's, not Thursday's.
    const answer = fantasyAnswer(LINEUP_SUBJECT, lineupWorld(onlyInvalid("empty"), THURSDAY - 4 * HOUR));
    expect(answer.band).toBe("near");
    expect(answer.collapsedHeadline).toBe("Twinion Family League · 1 empty slot — locks in 2d");
  });

  it("stale in crunch time: says how old the answer is rather than reading as current", () => {
    const answer = fantasyAnswer(LINEUP_SUBJECT, lineupWorld(GOLDEN_INVALID, THURSDAY - 4 * HOUR, 3 * HOUR));
    expect(answer.band).toBe("imminent");
    expect(answer.collapsedHeadline).toBe(
      "Twinion Family League · 3 slots to fix, last checked 3h ago — locks in 4h",
    );
    expect(answer.icon?.map((glyph) => glyph.kind === "icon" && glyph.label)).toContain("not checked recently");
  });
});

describe("the lineup's words", () => {
  it("names one shared reason, or counts slots when they differ", () => {
    expect(problemWords(2, "empty")).toBe("2 empty slots");
    expect(problemWords(1, "bye")).toBe("1 starter on bye");
    expect(problemWords(2, "out")).toBe("2 starters out");
    expect(problemWords(1, "injuredReserve")).toBe("1 starter on IR");
    expect(problemWords(3, null)).toBe("3 slots to fix");
    expect(problemWords(1, null)).toBe("1 slot to fix");
  });

  it("is short about durations and floors them", () => {
    expect(shortDuration(4 * HOUR + 50 * 60_000)).toBe("4h");
    expect(shortDuration(45 * 60_000)).toBe("45m");
    expect(shortDuration(10_000)).toBe("1m");
    expect(shortDuration(3 * 24 * HOUR)).toBe("3d");
  });

  it("sets a lock as a short weekday and a lowercase clock", () => {
    const sunday = new Date(2026, 9, 4, 13, 0, 0).getTime();
    expect(lockTime(sunday, new Date(2026, 9, 1, 9, 0, 0).getTime())).toBe("Sun 1:00pm");
    expect(lockTime(sunday, new Date(2026, 8, 20, 9, 0, 0).getTime())).toBe("Oct 4 1:00pm");
    // Already past: the date, never a weekday that reads as this week's.
    expect(lockTime(sunday, new Date(2026, 9, 5, 9, 0, 0).getTime())).toBe("Oct 4 1:00pm");
  });

  it("words every headline arm", () => {
    const now = new Date(2026, 9, 1, 9, 0, 0).getTime();
    const sunday = new Date(2026, 9, 4, 13, 0, 0).getTime();
    expect(lineupHeadline({ kind: "set", locksAtMs: sunday }, now)).toBe("Lineup set — locks Sun 1:00pm");
    expect(lineupHeadline({ kind: "set", locksAtMs: null }, now)).toBe("Lineup set");
    expect(
      lineupHeadline({ kind: "invalid", count: 2, reason: "empty", locksAtMs: now + 4 * HOUR, checkedAgoMs: null }, now),
    ).toBe("2 empty slots — locks in 4h");
    expect(
      lineupHeadline({ kind: "invalid", count: 2, reason: "empty", locksAtMs: null, checkedAgoMs: null }, now),
    ).toBe("2 empty slots");
    expect(
      lineupHeadline({ kind: "invalid", count: 1, reason: "bye", locksAtMs: now - 1, checkedAgoMs: null }, now),
    ).toBe("1 starter on bye — lock passed");
  });
});
