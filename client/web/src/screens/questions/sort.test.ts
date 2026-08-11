import { describe, expect, it } from "vitest";
import type { AnswerState, Band, PaneAnswer, RankedPane } from "./contract";
import { orderPanes, samePaneIdentity } from "./sort";

// ADR-0015's cross-pane sort (#245): five axes, in order, and a total order
// at the end of them — the same properties `frontier-order.test.ts` pins for
// the frontier's own sort.

const QUESTION_ORDER = ["alpha", "beta"];

function pane(overrides: {
  question?: string;
  subjectKey?: string;
  answerState?: AnswerState;
  band?: Band;
  withinBand?: number | null;
}): RankedPane {
  const answer: PaneAnswer = {
    answerState: overrides.answerState ?? "answered",
    band: overrides.band ?? "near",
    withinBand: overrides.withinBand === undefined ? 0 : overrides.withinBand,
    collapsedHeadline: "a line",
  };
  const question = (overrides.question ?? "alpha") as RankedPane["question"];
  const subjectKey = overrides.subjectKey ?? "subject";
  return { question, subjectKey, paneKey: `${question}:${subjectKey}`, answer };
}

function keys(panes: readonly RankedPane[]): string[] {
  return panes.map((ranked) => ranked.paneKey);
}

describe("orderPanes", () => {
  it("puts an answer ahead of a gap, and a gap ahead of an unbound question", () => {
    // An answer beats a promise, and a question nobody has asked yet is
    // setup rather than context — so it settles at the bottom.
    const ordered = orderPanes(
      [
        pane({ subjectKey: "unbound", answerState: "unbound" }),
        pane({ subjectKey: "gap", answerState: "bound-but-unacquired" }),
        pane({ subjectKey: "answered", answerState: "answered" }),
      ],
      QUESTION_ORDER,
    );
    expect(keys(ordered)).toEqual(["alpha:answered", "alpha:gap", "alpha:unbound"]);
  });

  it("orders by band before anything the panes say about themselves", () => {
    const ordered = orderPanes(
      [
        pane({ subjectKey: "dormant", band: "dormant" }),
        pane({ subjectKey: "distant", band: "distant" }),
        pane({ subjectKey: "live", band: "live" }),
        pane({ subjectKey: "near", band: "near" }),
        pane({ subjectKey: "imminent", band: "imminent" }),
      ],
      QUESTION_ORDER,
    );
    expect(keys(ordered)).toEqual([
      "alpha:live",
      "alpha:imminent",
      "alpha:near",
      "alpha:distant",
      "alpha:dormant",
    ]);
  });

  it("breaks a band tie by withinBand, soonest first", () => {
    const ordered = orderPanes(
      [
        pane({ subjectKey: "later", withinBand: 900 }),
        pane({ subjectKey: "sooner", withinBand: -50 }),
        pane({ subjectKey: "middle", withinBand: 10 }),
      ],
      QUESTION_ORDER,
    );
    expect(keys(ordered)).toEqual(["alpha:sooner", "alpha:middle", "alpha:later"]);
  });

  it("sorts a null withinBand after every real one — nothing to order by is not sooner", () => {
    const ordered = orderPanes(
      [
        pane({ subjectKey: "none", withinBand: null }),
        pane({ subjectKey: "huge", withinBand: Number.MAX_SAFE_INTEGER }),
      ],
      QUESTION_ORDER,
    );
    expect(keys(ordered)).toEqual(["alpha:huge", "alpha:none"]);
  });

  it("falls back to the declared question order, then the subject key", () => {
    const ordered = orderPanes(
      [
        pane({ question: "beta", subjectKey: "a" }),
        pane({ question: "alpha", subjectKey: "b" }),
        pane({ question: "alpha", subjectKey: "a" }),
      ],
      QUESTION_ORDER,
    );
    // Declared order beats alphabetical, and only then does the subject key
    // decide — which is what makes the whole order total.
    expect(keys(ordered)).toEqual(["alpha:a", "alpha:b", "beta:a"]);
  });

  it("never mutates its input and is byte-identical on a repeat call", () => {
    const input = [
      pane({ subjectKey: "b", band: "dormant" }),
      pane({ subjectKey: "a", band: "live" }),
    ];
    const before = keys(input);
    const once = orderPanes(input, QUESTION_ORDER);
    const twice = orderPanes(input, QUESTION_ORDER);
    expect(keys(input)).toEqual(before);
    expect(keys(once)).toEqual(keys(twice));
  });

  it("distinguishes an answered pane with nothing to say from a gap", () => {
    // Both may carry a null `withinBand` and a dormant band, so the only
    // thing separating them is `answerState` — if that axis were dropped the
    // two would sort interchangeably and read as the same state.
    const ordered = orderPanes(
      [
        pane({ subjectKey: "gap", answerState: "bound-but-unacquired", band: "dormant", withinBand: null }),
        pane({ subjectKey: "quiet", answerState: "answered", band: "dormant", withinBand: null }),
      ],
      QUESTION_ORDER,
    );
    expect(keys(ordered)).toEqual(["alpha:quiet", "alpha:gap"]);
  });
});

describe("samePaneIdentity", () => {
  it("is true when only band and withinBand moved", () => {
    // The whole point: the clock moving a pane's band must not re-open the
    // captured order under the reader's cursor.
    expect(
      samePaneIdentity(
        [pane({ subjectKey: "a", band: "dormant", withinBand: 900 })],
        [pane({ subjectKey: "a", band: "imminent", withinBand: 5 })],
      ),
    ).toBe(true);
  });

  it("is false when a pane appears, disappears, or changes answer state", () => {
    const base = [pane({ subjectKey: "a" })];
    expect(samePaneIdentity(base, [...base, pane({ subjectKey: "b" })])).toBe(false);
    expect(samePaneIdentity(base, [])).toBe(false);
    expect(
      samePaneIdentity(base, [pane({ subjectKey: "a", answerState: "bound-but-unacquired" })]),
    ).toBe(false);
  });

  it("compares by pane key, not by position", () => {
    expect(
      samePaneIdentity(
        [pane({ subjectKey: "a" }), pane({ subjectKey: "b" })],
        [pane({ subjectKey: "b" }), pane({ subjectKey: "a" })],
      ),
    ).toBe(true);
  });
});
