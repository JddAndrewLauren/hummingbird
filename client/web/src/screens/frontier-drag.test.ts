// @vitest-environment jsdom

// The gesture's own arithmetic (#801): the spring, the hit test, and the
// scroll correction. What a drop *writes* is not tested here — that is
// `hummingbird_core::decisions::frontier::drop_edits`'s own suite, reached
// through `frontier-columns.test.ts`'s wire-hop cases — and what the board
// does with a gesture is `NowScreen.test.tsx`'s. This file is the physics.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  beginDrag,
  cardAttrs,
  clamp,
  columnAttrs,
  hitPart,
  Spring,
  type DragHost,
  type FrozenPart,
} from "./frontier-drag";

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

// `beginDrag` driven directly, over a hand-built board. `NowScreen.test.tsx`
// owns the wiring — that the board's own columns and items reach it — and
// this owns the gesture's own contract, which is cheaper to state here: how
// often it asks the core, and that it tracks a finger arriving as a stream
// of small deltas rather than one jump.
describe("beginDrag", () => {
  // Reduced motion, so a release settles in the same tick rather than over
  // rAF frames vitest would have to be driven through. The gesture's own
  // decisions — which column, whether to write — are the same either way;
  // only the physics between them is skipped.
  beforeEach(() => {
    window.matchMedia = ((query: string) => ({
      matches: query.includes("prefers-reduced-motion"),
      media: query,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      onchange: null,
      dispatchEvent: () => false,
    })) as unknown as typeof window.matchMedia;
  });

  afterEach(() => {
    document.body.innerHTML = "";
    vi.restoreAllMocks();
  });

  /** Two columns side by side, one card in the first. jsdom lays nothing
   * out, so every box is stubbed by the element's own role. */
  function board() {
    document.body.innerHTML = "";
    const make = (key: string, left: number) => {
      const el = document.createElement("div");
      const attrs = columnAttrs(key, key);
      for (const [name, value] of Object.entries(attrs)) el.setAttribute(name, value);
      Object.assign(el.style, { position: "absolute" });
      document.body.append(el);
      el.getBoundingClientRect = () =>
        ({
          left,
          top: 0,
          right: left + 200,
          bottom: 400,
          width: 200,
          height: 400,
          x: left,
          y: 0,
          toJSON: () => ({}),
        }) as DOMRect;
      return el;
    };
    make("@phone", 0);
    make("@desk", 220);

    const card = document.createElement("div");
    for (const [name, value] of Object.entries(cardAttrs("i1"))) card.setAttribute(name, value);
    document.body.append(card);
    card.getBoundingClientRect = () =>
      ({
        left: 10,
        top: 10,
        right: 190,
        bottom: 70,
        width: 180,
        height: 60,
        x: 10,
        y: 10,
        toJSON: () => ({}),
      }) as DOMRect;
    return card;
  }

  function hostFor(allowedKeys: string[]) {
    const allowed = vi.fn(() => new Set(allowedKeys));
    const drop = vi.fn();
    const host: DragHost = {
      allowed,
      drop,
      scroller: () => null,
      onArm: () => {},
      onLand: () => {},
    };
    return { host, allowed, drop };
  }

  function press(card: HTMLElement, host: DragHost) {
    const down = new MouseEvent("pointerdown", { clientX: 20, clientY: 20 }) as PointerEvent;
    Object.assign(down, { pointerId: 1, pointerType: "mouse" });
    return beginDrag(card, "i1", down, host);
  }

  function moveTo(card: HTMLElement, x: number, y: number) {
    const move = new MouseEvent("pointermove", { clientX: x, clientY: y });
    Object.assign(move, { pointerId: 1, pointerType: "mouse" });
    card.dispatchEvent(move);
  }

  it("asks the core which columns it may land in exactly once per gesture", () => {
    // `dropEdits` is a wasm crossing plus two `JSON.stringify`s over the
    // whole project list; rule 1 in this module's header is that a gesture
    // does not do that per frame.
    const card = board();
    const { host, allowed } = hostFor(["@desk"]);
    press(card, host);
    for (let x = 30; x <= 300; x += 10) moveTo(card, x, 200);
    expect(allowed).toHaveBeenCalledTimes(1);
  });

  it("resolves the column from where the pointer is, across a stream of moves", () => {
    // The card is carried by where the pointer IS, never by an accumulation
    // of where it has been. (The failure that rule exists for is Android's
    // — an integrator fed its own lagging output falls progressively behind
    // and never reaches the column being aimed at; `BoardDragGestureTest`
    // is where a multi-step drag pins it.)
    const card = board();
    const { host, drop } = hostFor(["@desk"]);
    press(card, host);
    for (let x = 30; x <= 300; x += 10) moveTo(card, x, 200);
    const up = new MouseEvent("pointerup", { clientX: 300, clientY: 200 });
    Object.assign(up, { pointerId: 1, pointerType: "mouse" });
    card.dispatchEvent(up);
    expect(drop).toHaveBeenCalledWith("@desk");
  });

  it("writes nothing when the gesture is aborted mid-flight", () => {
    const card = board();
    const { host, drop } = hostFor(["@desk"]);
    const live = press(card, host);
    for (let x = 30; x <= 300; x += 10) moveTo(card, x, 200);
    live.abort();
    const up = new MouseEvent("pointerup", { clientX: 300, clientY: 200 });
    Object.assign(up, { pointerId: 1, pointerType: "mouse" });
    card.dispatchEvent(up);
    expect(drop).not.toHaveBeenCalled();
    expect(card.style.transform).toBe("");
  });
});
