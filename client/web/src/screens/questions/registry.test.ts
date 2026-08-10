import { describe, expect, it } from "vitest";
import { QUESTION_ORDER, boundedGlyphs, paneKey, type PaneGlyph, type QuestionDef, type QuestionInputs } from "./contract";
import { orderPanes } from "./sort";
import { QUESTIONS, rankPanes, requiredSources } from "./registry";

// The registry, and the two properties that keep it honest: it names every
// question exactly once, and it ranks every subject of every question — not
// just the one-subject case the only shipped pane happens to be.

function emptyInputs(): QuestionInputs {
  return { bindings: null, paneReads: {}, nowMs: 1_000 };
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

describe("rankPanes", () => {
  it("still emits a pane for a question nobody has bound", () => {
    // The setup prompt is how the question is discovered at all — a
    // registry that dropped unbound questions would hide every one of them
    // from the person who has to configure them.
    const panes = rankPanes(emptyInputs());
    expect(panes.length).toBeGreaterThan(0);
    expect(panes.every((pane) => pane.answer.answerState === "unbound")).toBe(true);
  });

  it("keys each pane by question and subject, never by position", () => {
    for (const pane of rankPanes(emptyInputs())) {
      expect(pane.paneKey).toBe(paneKey(pane.question, pane.subjectKey));
    }
  });

  it("ranks every subject of a multi-subject question", () => {
    // No shipped question emits more than one subject yet, so the 0..N
    // contract is exercised here rather than left to be discovered by the
    // first question that does.
    const twoSubjects: QuestionDef = {
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
    const panes = ["b", "a"].map((subjectKey) => ({
      question: "test" as never,
      subjectKey,
      paneKey: `test:${subjectKey}`,
      answer: twoSubjects.answer(subjectKey, emptyInputs()),
    }));

    // Both subjects survive, and `withinBand` orders them rather than the
    // order `subjects()` happened to return.
    expect(orderPanes(panes, ["test"]).map((pane) => pane.subjectKey)).toEqual(["a", "b"]);
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
