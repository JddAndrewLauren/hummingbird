import { describe, expect, it } from "vitest";
import {
  QUESTION_ORDER,
  paneKey,
  type RankedPane,
} from "../questions/contract";
import { QUESTIONS } from "../questions/registry";
import { STATUS_GROUPS, tileGroup, tileIcon } from "./tile-vocabulary";

function pane(
  question: RankedPane["question"],
  subjectKey: string,
): RankedPane {
  return {
    question,
    subjectKey,
    paneKey: paneKey(question, subjectKey),
    answer: {
      answerState: "answered",
      band: "dormant",
      withinBand: null,
      collapsedHeadline: `${subjectKey} · healthy`,
    },
  };
}

const STATUS_QUESTIONS = QUESTION_ORDER.filter(
  (q) => QUESTIONS[q].surface === "status",
);

describe("the Status board's tile vocabulary", () => {
  // The drift pin. A question added to the status surface without a place on
  // the board would silently land in `infra` with a neutral glyph — legible,
  // but not a decision anyone made, which is what this catches.
  it("gives every registered status question a group and a glyph of its own", () => {
    expect(STATUS_QUESTIONS.length).toBeGreaterThan(0);
    for (const question of STATUS_QUESTIONS) {
      const drawn = pane(question, "subject");
      expect(STATUS_GROUPS).toContain(tileGroup(drawn));
      expect(tileIcon(drawn)).not.toBe("activity");
    }
  });

  it("puts no 'now' question on the board", () => {
    const nowQuestions = QUESTION_ORDER.filter(
      (q) => QUESTIONS[q].surface === "now",
    );
    expect(nowQuestions.length).toBeGreaterThan(0);
    // Reaching the neutral fallback is how an unregistered question reads —
    // which is what every Now question must be here.
    for (const question of nowQuestions) {
      expect(tileIcon(pane(question, "subject"))).toBe("activity");
    }
  });

  it("uses both groups, so the board is never one unlabelled grid", () => {
    const groups = new Set(
      STATUS_QUESTIONS.map((q) => tileGroup(pane(q, "subject"))),
    );
    expect(groups.size).toBe(STATUS_GROUPS.length);
  });

  it("gives each uptime service its own glyph", () => {
    expect(tileIcon(pane("uptime", "authority"))).toBe("server");
    expect(tileIcon(pane("uptime", "web"))).toBe("globe");
    expect(tileIcon(pane("uptime", "runner"))).toBe("cpu");
  });

  it("gives a workflow the glyph of the source it polls", () => {
    expect(tileIcon(pane("github", "race-alert-poll.yml"))).toBe("flag");
    expect(tileIcon(pane("github", "city-waste-poll.yml"))).toBe("trash-2");
  });

  // Subject keys are server data — a renamed workflow, a newly probed
  // service — so they cannot be pinned. What is pinned is that an
  // unrecognised one still draws, with its question's own glyph.
  it("falls back to the question's glyph for a subject it has never heard of", () => {
    expect(
      tileIcon(pane("github", "a-workflow-nobody-has-added-yet.yml")),
    ).toBe("git-branch");
    expect(tileIcon(pane("uptime", "some-new-probe"))).toBe("server");
    expect(
      tileGroup(pane("github", "a-workflow-nobody-has-added-yet.yml")),
    ).toBe("capture & context sources");
  });
});
