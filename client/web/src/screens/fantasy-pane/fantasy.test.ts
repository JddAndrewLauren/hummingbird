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
} from "./fantasy";

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

  it("a row this build cannot read yet is a gap, never an answer", () => {
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
    expect(fantasyGapReason("449.l.1:lineup", world)).toMatch(/does not read yet/);
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
