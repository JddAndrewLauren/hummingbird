// The lane packing, tested where it is decidable: pure numbers in, indices
// out. The board itself cannot prove any of this — jsdom lays nothing out, so
// a component test sees the unmeasured branch and would assert the packing
// vacuously if it tried. That split is deliberate and is the same one
// `useIsPhone.ts`'s header describes for the viewport: the runtime question is
// answered where it can be answered, and everything downstream of it is a
// function anyone can call.

import { describe, expect, it } from "vitest";
import { frontierLanes, laneCountFor, packLanes } from "./frontier-lanes";

describe("laneCountFor", () => {
  it("gives every column its own lane when the width is unknown", () => {
    // The pre-lanes layout, and the only honest answer for a runtime that
    // cannot lay out. A jsdom test therefore keeps asserting the board it
    // always asserted.
    expect(laneCountFor(null, 5)).toBe(5);
    expect(laneCountFor(null, 1)).toBe(1);
  });

  it("has no lanes when there are no columns, measured or not", () => {
    expect(laneCountFor(null, 0)).toBe(0);
    expect(laneCountFor(1200, 0)).toBe(0);
  });

  it("fits as many 240px lanes as the width and its gaps allow", () => {
    // n lanes cost n*240 + (n-1)*24.
    expect(laneCountFor(240, 9)).toBe(1);
    expect(laneCountFor(503, 9)).toBe(1);
    expect(laneCountFor(504, 9)).toBe(2);
    expect(laneCountFor(767, 9)).toBe(2);
    expect(laneCountFor(768, 9)).toBe(3);
    expect(laneCountFor(1032, 9)).toBe(4);
  });

  it("never opens a lane it has no column for", () => {
    expect(laneCountFor(1600, 2)).toBe(2);
  });

  it("keeps one lane on a phone, where nothing fits the minimum", () => {
    // A floor rather than zero: below the minimum the columns still have to
    // be drawn somewhere, and one full-bleed stack is what a phone wants.
    expect(laneCountFor(390, 4)).toBe(1);
    expect(laneCountFor(1, 4)).toBe(1);
  });
});

describe("packLanes", () => {
  it("fans the fullest columns across the lanes before stacking anything", () => {
    // A heavy column carries its whole share on its own, so the next one
    // starts a fresh lane: on a fullest-first axis the columns still read left
    // to right along the top, exactly as the wrapping row put them.
    expect(packLanes([9, 8, 7], 3)).toEqual([[0], [1], [2]]);
  });

  it("stacks the short columns under the lane already open", () => {
    // The paper cut: `@phone` is tall, `@home` and `@errands` hold one item
    // each, and the wrapping row gave each of them a full track.
    expect(packLanes([9, 2, 2, 2], 2)).toEqual([
      [0],
      [1, 2, 3],
    ]);
  });

  it("stacks the sparse bands the urgency axis leads with", () => {
    // The shape this packing exists for, and the one greedy-balance got
    // wrong: `group_frontier` hands over overdue/now/soon/calm in SEVERITY
    // order, so the three slight columns arrive first. Balancing gave each of
    // them a lane of its own and put `calm` under the first; filling stacks
    // the three and leaves `calm` a lane to itself. Weights are the board's
    // own at the 1440 capture — a card each for the bands, `COLUMN_CAP` plus
    // a header and an `n more` row for `calm`.
    expect(packLanes([2, 2, 2, 8], 3)).toEqual([
      [0, 1, 2],
      [3],
    ]);
  });

  it("aims no lane lower than the tallest single column", () => {
    // Without that floor the even share is 14/3, and `soon` is pulled out of
    // the severity stack into a lane of its own — three lanes where two hold
    // the same board at the same height, because `calm` is drawn whole
    // whatever happens and is 8 rows on its own.
    expect(packLanes([2, 2, 2, 8], 3)).toEqual([
      [0, 1, 2],
      [3],
    ]);
  });

  it("draws no lane the weights never reached", () => {
    // Fewer lanes than asked for, so the survivors widen instead of standing
    // as empty 240px flex items. Three afforded, two drawn.
    expect(packLanes([2, 2, 2, 8], 3)).toHaveLength(2);
    expect(packLanes([5, 1, 1], 3)).toEqual([
      [0],
      [1, 2],
    ]);
  });

  it("keeps a lumpy fullest-first axis on all the lanes it affords", () => {
    // The context axis at the 1440 capture, as rendered rows: @computer
    // capped at 6, then @errands, @phone, @homework, @home, and the no-value
    // column last. Without the look-ahead the first lane swallowed @errands,
    // ran to 13 against a 10.3 share, and the third lane was never opened at
    // all — the whole board on two lanes, one of them half again as tall.
    expect(packLanes([8, 5, 4, 3, 3, 8], 3)).toEqual([
      [0],
      [1, 2],
      [3, 4, 5],
    ]);
  });

  it("splits a run of equal columns in reading order", () => {
    // The accepted cost of filling rather than balancing, spelled out: these
    // read 1,2 down the left lane and 3,4 down the right, where balancing
    // interleaved them 1,3 / 2,4. Nothing here is far enough down a lane to
    // be missed — the module header says so.
    expect(packLanes([1, 1, 1, 1], 2)).toEqual([
      [0, 1],
      [2, 3],
    ]);
  });

  it("preserves the given order outright in a single lane", () => {
    // The phone case, and the unmeasured-width case's mirror: one lane must
    // be the stack `group_frontier` handed over, never a reordering of it.
    expect(packLanes([1, 9, 3], 1)).toEqual([[0, 1, 2]]);
  });

  it("gives every column its own lane when each fills one", () => {
    expect(packLanes([1, 1, 1], 3)).toEqual([[0], [1], [2]]);
  });

  it("returns no lanes for no lanes, and none for no columns", () => {
    expect(packLanes([1, 2], 0)).toEqual([]);
    expect(packLanes([], 3)).toEqual([]);
  });
});

describe("frontierLanes", () => {
  it("gives every column its own lane when the width is unknown", () => {
    // The pre-lanes board, answered here rather than through the packer: a
    // runtime that cannot lay out has not chosen a packing, and a jsdom
    // component test keeps asserting the structure it always asserted.
    expect(frontierLanes([2, 2, 2, 8], null)).toEqual([[0], [1], [2], [3]]);
    expect(frontierLanes([], null)).toEqual([]);
  });

  it("packs into the lanes the measured width affords", () => {
    // 830px is about what Now's centre column measures at the 1440 capture:
    // three lanes afforded, two drawn.
    expect(frontierLanes([2, 2, 2, 8], 830)).toEqual([
      [0, 1, 2],
      [3],
    ]);
  });

  it("stacks the whole board in one lane on a phone", () => {
    expect(frontierLanes([2, 2, 2, 8], 390)).toEqual([[0, 1, 2, 3]]);
  });
});
