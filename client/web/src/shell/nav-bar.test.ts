import { describe, expect, it } from "vitest";
import { NAV_BAR_OVERFLOW, NAV_BAR_PRIMARY, isOverflowScreen } from "./nav-bar";
import { SCREENS } from "./screens";

describe("the phone nav partition", () => {
  // The one that matters: a screen added to `SCREENS` must land somewhere.
  // The failure this prevents is silent — a new surface reachable from the
  // rail on a desktop and from nowhere at all on a phone.
  it("the two halves reconstruct SCREENS exactly, with nothing lost or doubled", () => {
    expect([...NAV_BAR_PRIMARY, ...NAV_BAR_OVERFLOW].sort()).toEqual([...SCREENS].sort());
    expect(new Set([...NAV_BAR_PRIMARY, ...NAV_BAR_OVERFLOW]).size).toBe(SCREENS.length);
  });

  it("each half keeps SCREENS' own order", () => {
    expect(NAV_BAR_PRIMARY).toEqual(SCREENS.filter((s) => NAV_BAR_PRIMARY.includes(s)));
    expect(NAV_BAR_OVERFLOW).toEqual(SCREENS.filter((s) => NAV_BAR_OVERFLOW.includes(s)));
  });

  // Four, against --touch-min: 44px at 390px. Five would not fit with gaps,
  // and the number is the whole reason the sheet exists — so it is pinned,
  // not left to be widened by accident.
  it("the bar carries exactly four screens", () => {
    expect(NAV_BAR_PRIMARY).toHaveLength(4);
  });

  it("More reads as current exactly when the open screen is one it hides", () => {
    for (const screen of NAV_BAR_PRIMARY) {
      expect(isOverflowScreen(screen)).toBe(false);
    }
    for (const screen of NAV_BAR_OVERFLOW) {
      expect(isOverflowScreen(screen)).toBe(true);
    }
  });
});
