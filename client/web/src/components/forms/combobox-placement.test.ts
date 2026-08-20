import { describe, expect, it } from "vitest";
import { popupPlacement } from "./combobox-placement";

const METRICS = { gap: 4, margin: 8, preferredHeight: 240 };

/** A field 36px tall, 200px wide, with its top at `top`. */
function field(top: number) {
  return { top, bottom: top + 36, left: 120, width: 200 };
}

describe("popupPlacement", () => {
  it("hangs under the field, at its own width, when there is room", () => {
    expect(popupPlacement(field(100), 900, METRICS)).toEqual({
      top: 140,
      left: 120,
      width: 200,
      maxHeight: 240,
      above: false,
    });
  });

  it("flips above when the field sits near the foot of the viewport", () => {
    const placed = popupPlacement(field(700), 844, METRICS);
    expect(placed.above).toBe(true);
    // Its foot is `gap` above the field's head, and it is as tall as the room
    // there allows.
    expect(placed.top + placed.maxHeight).toBe(696);
    expect(placed.maxHeight).toBe(240);
  });

  it("shrinks rather than running off the edge it landed on", () => {
    // 100px of room below, 48 above: neither fits 240, below still wins, and
    // the popup takes exactly the 100 it has.
    const placed = popupPlacement(field(60), 208, METRICS);
    expect(placed).toMatchObject({ above: false, top: 100, maxHeight: 100 });
  });

  it("keeps a near-tie downward", () => {
    // Equal room either side, and neither side fits the preferred height.
    const placed = popupPlacement(field(150), 336, METRICS);
    expect(placed.above).toBe(false);
  });

  it("never returns a negative height", () => {
    const placed = popupPlacement(field(-400), 300, METRICS);
    expect(placed.maxHeight).toBeGreaterThanOrEqual(0);
  });
});
