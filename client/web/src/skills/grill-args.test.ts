import { describe, expect, it } from "vitest";
import { formatGrillTranscript, grillRunBody } from "./grill-args";

describe("grillRunBody", () => {
  it("opens an interview with an empty turns array", () => {
    expect(grillRunBody({ ref: "8f2c-uuid", turns: [] })).toEqual({
      skill: "grill-me",
      args: { ref: "8f2c-uuid", turns: [] },
    });
  });

  it("threads the whole conversation back, never a trimmed subset", () => {
    const turns = [
      {
        question: { prompt: "Which airport?", recommendedAnswer: "SEA", choices: ["SEA", "PDX"] },
        answer: "SEA",
      },
      {
        question: { prompt: "Which dates?", recommendedAnswer: "next week", choices: ["next week", "in a month"] },
        answer: "the third week of September",
      },
    ];
    expect(grillRunBody({ ref: "i", turns }).args.turns).toEqual(turns);
  });

  /** #355's brief asks for no comforts beyond the interview — no `model`
   * option, unlike `microtaskRunBody`. */
  it("carries no model option — the args are exactly ref and turns", () => {
    expect(Object.keys(grillRunBody({ ref: "i", turns: [] }).args)).toEqual(["ref", "turns"]);
  });
});

describe("formatGrillTranscript", () => {
  it("is the empty string for no turns", () => {
    expect(formatGrillTranscript([])).toBe("");
  });

  it("lists each round as a Q/A pair, in order", () => {
    const turns = [
      {
        question: { prompt: "Which airport?", recommendedAnswer: "SEA", choices: ["SEA", "PDX"] },
        answer: "SEA",
      },
      {
        question: { prompt: "Which dates?", recommendedAnswer: "next week", choices: ["next week", "in a month"] },
        answer: "the third week of September",
      },
    ];
    expect(formatGrillTranscript(turns)).toBe(
      "Q: Which airport?\nA: SEA\n\nQ: Which dates?\nA: the third week of September",
    );
  });
});
