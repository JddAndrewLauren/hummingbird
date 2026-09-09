// The gesture's own arithmetic (#801): the spring, the hit test, and the
// scroll correction. What a drop *writes* is not tested here — that is
// `hummingbird_core::decisions::frontier::drop_edits`'s own suite, reached
// through `frontier-columns.test.ts`'s wire-hop cases — and what the board
// does with a gesture is `NowScreen.test.tsx`'s. This file is the physics.

import { describe, expect, it } from "vitest";
import { clamp, hitPart, Spring, type FrozenPart } from "./frontier-drag";

function part(key: string, left: number, top: number, width = 200, height = 400): FrozenPart {
  return {
    key,
    value: key === "" ? null : key,
    rect: {
      left,
      top,
      right: left + width,
      bottom: top + height,
      width,
      height,
      x: left,
      y: top,
      toJSON: () => ({}),
    } as DOMRect,
  };
}

describe("Spring", () => {
  it("comes to rest at its target", () => {
    const s = new Spring(0);
    s.target = 100;
    for (let frame = 0; frame < 200 && !s.resting; frame += 1) s.step(16);
    expect(s.resting).toBe(true);
    expect(s.x).toBeCloseTo(100, 0);
  });

  it("overshoots on the way — the under-damping is the gesture's character", () => {
    const s = new Spring(0);
    s.target = 100;
    let peak = 0;
    for (let frame = 0; frame < 200; frame += 1) {
      s.step(16);
      peak = Math.max(peak, s.x);
    }
    expect(peak).toBeGreaterThan(100);
  });

  it("survives the one enormous frame a backgrounded tab hands back", () => {
    const s = new Spring(0);
    s.target = 100;
    s.step(4000);
    expect(Number.isFinite(s.x)).toBe(true);
    // The step was capped at 32ms, so one frame cannot fling the card past
    // its target and out of the document.
    expect(Math.abs(s.x)).toBeLessThan(100);
  });

  it("snaps without motion, for a reader who asked for less of it", () => {
    const s = new Spring(0);
    s.v = 900;
    s.snap(42);
    expect([s.x, s.target, s.v]).toEqual([42, 42, 0]);
  });
});

describe("hitPart", () => {
  const columns = [part("@phone", 0, 0), part("@desk", 220, 0), part("", 440, 0)];

  it("answers the column a point is inside", () => {
    expect(hitPart(columns, { x: 300, y: 200 })?.key).toBe("@desk");
    expect(hitPart(columns, { x: 500, y: 10 })?.value).toBeNull();
  });

  it("resolves a release in the gutter between two lanes", () => {
    // The gutter is dead ground the reader cannot see; a hand that lets go in
    // one is pointing at the column beside it, not at nothing.
    expect(hitPart(columns, { x: 210, y: 200 })?.key).toBe("@phone");
  });

  it("answers nothing beyond the slack, so the card springs home", () => {
    expect(hitPart(columns, { x: 900, y: 900 })).toBeNull();
    expect(hitPart([], { x: 10, y: 10 })).toBeNull();
  });

  it("hit-tests a scrolled board against the frame the rects were frozen in", () => {
    // The columns were measured before the container scrolled 300px; the
    // pointer is at viewport y=50, which is y=350 in that frozen frame — the
    // second row of the column, not above the board.
    const scrolled = 300;
    expect(hitPart(columns, { x: 100, y: 50 + scrolled })?.key).toBe("@phone");
    // Without the correction the same reading falls off the top of the board
    // by more than the slack.
    expect(hitPart(columns, { x: 100, y: -60 })).toBeNull();
  });
});

describe("clamp", () => {
  it("bounds the tilt so a carried card never reads as spin", () => {
    expect([clamp(-900, -8, 8), clamp(0, -8, 8), clamp(900, -8, 8)]).toEqual([-8, 0, 8]);
  });
});
