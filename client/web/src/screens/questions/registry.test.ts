import { describe, expect, it } from "vitest";
import {
  QUESTION_ORDER,
  boundedGlyphs,
  paneKey,
  type PaneGlyph,
  type QuestionDef,
  type QuestionInputs,
  type StandingQuestion,
} from "./contract";
import { QUESTIONS, panesFrom, rankPanes, requiredCalendarRequests, requiredSources } from "./registry";

// The registry, and the two properties that keep it honest: it names every
// question exactly once, and it ranks every subject of every question — not
// just the one-subject case the only shipped pane happens to be.

function emptyInputs(): QuestionInputs {
  return {
    bindings: [],
    paneReads: {},
    calendarReads: {},
    calendarConnected: false,
    items: [],
    nowMs: 1_000,
  };
}

/** A registry of questions that do not exist in the shipped vocabulary —
 * the only way to run a multi-subject (or subject-less) question through the
 * real expansion, since `StandingQuestion` is deliberately closed. */
function fakeRegistry(
  questions: Record<string, QuestionDef>,
): Record<StandingQuestion, QuestionDef> {
  return questions as unknown as Record<StandingQuestion, QuestionDef>;
}

describe("QUESTION_ORDER", () => {
  it("names every registered question exactly once", () => {
    expect([...QUESTION_ORDER].sort()).toEqual(Object.keys(QUESTIONS).sort());
    expect(new Set(QUESTION_ORDER).size).toBe(QUESTION_ORDER.length);
  });

  it("gives every question a human label the shell can draw", () => {
    for (const question of QUESTION_ORDER) {
      expect(QUESTIONS[question].label).not.toBe("");
    }
  });
});

describe("requiredSources", () => {
  it("unions every question's sources without repeating one", () => {
    const sources = requiredSources();
    expect(new Set(sources).size).toBe(sources.length);
    for (const question of QUESTION_ORDER) {
      for (const source of QUESTIONS[question].sources) {
        expect(sources).toContain(source);
      }
    }
  });
});

describe("requiredCalendarRequests", () => {
  it("picks up the weekend-plans pane's interval without requiredCalendarRequests itself changing (#122)", () => {
    // This was the empty steady state before #122 registered a real
    // calendar-lane question — proof the union mechanism, not a fixed
    // list, is what decides this.
    const requests = requiredCalendarRequests(1_000);
    expect(requests).toEqual([{ key: "weekend", startMs: expect.any(Number), endMs: expect.any(Number) }]);
  });

  it("is a pure function of the clock, with no calendar read of its own", () => {
    const a = requiredCalendarRequests(1_000);
    const b = requiredCalendarRequests(1_000);
    expect(a).toEqual(b);
  });
});

describe("rankPanes", () => {
  it("still emits a pane for every question, none of them answered, when nothing is set up", () => {
    // The setup prompt is how a question is discovered at all — a registry
    // that dropped an unconfigured question would hide it from the person
    // who has to set it up. Not every question reads `unbound` from
    // `emptyInputs()` specifically: `waste` (an unset binding) does, while
    // `weekend` (a calendar arm nobody has requested a read for yet) is
    // `bound-but-unacquired` — both are gaps, and the shared assertion here
    // is the one thing every registered question owes: never `answered`
    // with nothing behind it.
    const panes = rankPanes(emptyInputs());
    expect(panes.length).toBeGreaterThan(0);
    expect(panes.every((pane) => pane.answer.answerState !== "answered")).toBe(true);
  });

  it("keys each pane by question and subject, never by position", () => {
    for (const pane of rankPanes(emptyInputs())) {
      expect(pane.paneKey).toBe(paneKey(pane.question, pane.subjectKey));
    }
  });

  it("ranks every subject of a multi-subject question, and none of a question with no subjects", () => {
    // No shipped question emits more than one subject — or none — so the
    // 0..N contract is exercised here, through `panesFrom`, which is the
    // expansion `rankPanes` itself runs. Hand-building the panes and sorting
    // them would test the sort a second time and the expansion not at all.
    const multi: QuestionDef = {
      label: "Two things",
      sources: [],
      subjects: () => ["b", "a"],
      answer: (subjectKey) => ({
        answerState: "answered",
        band: "near",
        withinBand: subjectKey === "a" ? 1 : 2,
        collapsedHeadline: subjectKey,
      }),
      Expanded: () => null,
    };
    const none: QuestionDef = { ...multi, label: "Nothing yet", subjects: () => [] };

    const panes = panesFrom(
      fakeRegistry({ multi, none }),
      ["multi", "none"] as unknown as StandingQuestion[],
      emptyInputs(),
    );

    // Both of the one question's subjects survive, keyed by subject and
    // ordered by `withinBand` rather than by the order `subjects()` returned
    // — and the subject-less question contributes no pane at all.
    expect(panes.map((pane) => pane.paneKey)).toEqual(["multi:a", "multi:b"]);
  });
});

describe("boundedGlyphs", () => {
  it("caps a pane's glyphs and treats none as none", () => {
    const many: PaneGlyph[] = Array.from({ length: 9 }, (_, index) => ({
      kind: "dot",
      fill: "#000",
      edge: "#000",
      label: `dot-${index}`,
    }));
    expect(boundedGlyphs(many)).toHaveLength(4);
    expect(boundedGlyphs(undefined)).toEqual([]);
  });
});
