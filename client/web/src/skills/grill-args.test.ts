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

  it("model rides along when set", () => {
    expect(grillRunBody({ ref: "i", turns: [], model: "opus" })).toEqual({
      skill: "grill-me",
      args: { ref: "i", turns: [], model: "opus" },
    });
  });

  /** Same rule `microtaskRunBody` applies: the empty value is the "Default
   * model" option, and omitting the key is what leaves the runner's own
   * default in place rather than naming it twice. */
  it("an empty model omits the key entirely", () => {
    expect("model" in grillRunBody({ ref: "i", turns: [], model: "" }).args).toBe(false);
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
