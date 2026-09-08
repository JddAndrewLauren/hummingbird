// The lane packing, tested where it is decidable: pure numbers in, indices
// out. The board itself cannot prove any of this — jsdom lays nothing out, so
// a component test sees the unmeasured branch and would assert the packing
// vacuously if it tried. That split is deliberate and is the same one
// `useIsPhone.ts`'s header describes for the viewport: the runtime question is
// answered where it can be answered, and everything downstream of it is a
// function anyone can call.

import { describe, expect, it } from "vitest";
import { columnCapFor, frontierLanes, laneCountFor, packLanes } from "./frontier-lanes";

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

  it("does not swallow a column that dwarfs the ones already in the lane", () => {
    // `calm` expanded: 29 cards and a header against three bands of one. The
    // share floors at 31, and a rule aiming merely CLOSEST to it put the whole
    // board in one lane — 37 is nearer 31 than 6 is. Past the share opens the
    // next lane instead, so expanding a column never relays the board.
    expect(packLanes([2, 2, 2, 31], 3)).toEqual([
      [0, 1, 2],
      [3],
    ]);
    // The same shape at two columns, where the floor and the even split are
    // furthest apart.
    expect(packLanes([1, 8], 2)).toEqual([[0], [1]]);
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
    // component test keeps asserting the structure it always asserted. It has
    // no width to spare either — spare width is a measurement.
    expect(frontierLanes([2, 2, 2, 8], null)).toEqual({
      lanes: [[0], [1], [2], [3]],
      spare: null,
    });
    expect(frontierLanes([], null)).toEqual({ lanes: [], spare: null });
  });

  it("packs into the lanes the measured width affords", () => {
    // 830px is about what Now's centre column measures at the 1440 capture:
    // three lanes afforded, two drawn, and the third is `calm`'s to run on
    // into — it is alone in the last packed lane.
    expect(frontierLanes([2, 2, 2, 8], 830)).toEqual({
      lanes: [[0, 1, 2], [3]],
      spare: { column: 3, lanes: 1 },
    });
  });

  it("offers no spare width when the packing used it all", () => {
    // The context axis at the same width: three lanes afforded, three drawn.
    expect(frontierLanes([8, 5, 4, 3, 3, 8], 830).spare).toBeNull();
  });

  it("offers the spare width to nobody when the last lane holds a stack", () => {
    // A continuation has to sit immediately right of what it continues. The
    // last lane here holds three columns and so has no single subject.
    expect(frontierLanes([9, 1, 1, 1], 1400)).toEqual({
      lanes: [[0], [1, 2, 3]],
      spare: null,
    });
  });

  it("stacks the whole board in one lane on a phone, with nothing to spare", () => {
    expect(frontierLanes([2, 2, 2, 8], 390)).toEqual({
      lanes: [[0, 1, 2, 3]],
      spare: null,
    });
  });
});

describe("columnCapFor", () => {
  it("keeps the board's long-standing cap when the height is unknown", () => {
    // jsdom, and the first paint before anything is measured. A component
    // test asserts the same six cards it always asserted.
    expect(columnCapFor(null)).toBe(6);
  });

  it("fills the room the board actually has", () => {
    // The fault this closes: at a 1400px viewport the urgency board stopped
    // 638px above the fold with 23 items behind its "n more". 1162px of room
    // is a heading, the gutter, and fourteen 78px cards.
    expect(columnCapFor(1162)).toBe(14);
    // 900px viewport, same board: about half that.
    expect(columnCapFor(662)).toBe(7);
  });

  it("shows a column rather than a stub on a short viewport", () => {
    // The floor. A laptop with the capture popover open still gets something
    // worth reading under the heading.
    expect(columnCapFor(300)).toBe(4);
    expect(columnCapFor(0)).toBe(4);
  });

  it("grows by one card for one card's worth of room", () => {
    expect(columnCapFor(68 + 78 * 5)).toBe(5);
    expect(columnCapFor(68 + 78 * 6)).toBe(6);
  });
});
