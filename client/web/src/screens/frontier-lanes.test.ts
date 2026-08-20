// The lane packing, tested where it is decidable: pure numbers in, indices
// out. The board itself cannot prove any of this — jsdom lays nothing out, so
// a component test sees the unmeasured branch and would assert the packing
// vacuously if it tried. That split is deliberate and is the same one
// `useIsPhone.ts`'s header describes for the viewport: the runtime question is
// answered where it can be answered, and everything downstream of it is a
// function anyone can call.

import { describe, expect, it } from "vitest";
import { laneCountFor, packLanes } from "./frontier-lanes";

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
  it("fans the first columns across the lanes before stacking anything", () => {
    // Every lane starts empty, so the fullest columns — which arrive first —
    // read left to right along the top, exactly as the wrapping row put them.
    expect(packLanes([9, 8, 7], 3)).toEqual([[0], [1], [2]]);
  });

  it("stacks the short columns under whichever lane is shortest", () => {
    // The paper cut: `@phone` is tall, `@home` and `@errands` hold one item
    // each, and the wrapping row gave each of them a full track.
    expect(packLanes([9, 2, 2, 2], 2)).toEqual([
      [0],
      [1, 2, 3],
    ]);
  });

  it("breaks a tie leftwards", () => {
    expect(packLanes([1, 1, 1, 1], 2)).toEqual([
      [0, 2],
      [1, 3],
    ]);
  });

  it("preserves the given order outright in a single lane", () => {
    // The phone case, and the unmeasured-width case's mirror: one lane must
    // be the stack `group_frontier` handed over, never a reordering of it.
    expect(packLanes([1, 9, 3], 1)).toEqual([[0, 1, 2]]);
  });

  it("returns one lane per column when asked for exactly that", () => {
    // What `laneCountFor(null, n)` produces, spelled out: the pre-lanes board.
    expect(packLanes([5, 1, 1], 3)).toEqual([[0], [1], [2]]);
  });

  it("returns no lanes for no lanes, whatever the weights", () => {
    expect(packLanes([1, 2], 0)).toEqual([]);
    expect(packLanes([], 3)).toEqual([[], [], []]);
  });
});
