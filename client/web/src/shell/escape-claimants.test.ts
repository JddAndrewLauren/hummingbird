import { describe, expect, it } from "vitest";
import {
  ESCAPE_CLAIMANTS,
  escapeClaimant,
  type EscapeClaimant,
  type EscapeInput,
} from "./escape-claimants";

function escape(open: Partial<Record<EscapeClaimant, boolean>> = {}): EscapeInput {
  return {
    key: "Escape",
    isComposing: false,
    open: { capture: false, search: false, navSheet: false, itemDetail: false, ...open },
  };
}

describe("escapeClaimant", () => {
  it("gives the keystroke to the only thing open", () => {
    for (const claimant of ESCAPE_CLAIMANTS) {
      expect(escapeClaimant(escape({ [claimant]: true }))).toBe(claimant);
    }
  });

  it("does nothing when nothing is open to close", () => {
    expect(escapeClaimant(escape())).toBeNull();
  });

  // The bug this module exists for: on a phone with a hardware keyboard, the
  // sheet and an item panel were both open and one Escape closed both, because
  // each bound its own document listener and neither could see the other.
  it("closes exactly one thing when every claimant is open", () => {
    expect(
      escapeClaimant(
        escape({ capture: true, search: true, navSheet: true, itemDetail: true }),
      ),
    ).toBe("search");
  });

  // Every pair, so the order is pinned rather than sampled — a later
  // reordering of `ESCAPE_CLAIMANTS` has to break a named case here.
  it.each([
    [["search", "capture"], "search"],
    [["capture", "navSheet"], "capture"],
    [["capture", "itemDetail"], "capture"],
    [["search", "navSheet"], "search"],
    [["search", "itemDetail"], "search"],
    [["navSheet", "itemDetail"], "navSheet"],
  ] as const)("with %s open, %s wins", (open, winner) => {
    const flags = Object.fromEntries(open.map((claimant) => [claimant, true]));
    expect(escapeClaimant(escape(flags))).toBe(winner);
  });

  it("leaves an IME composition's Escape to the composition", () => {
    expect(escapeClaimant({ ...escape({ capture: true }), isComposing: true })).toBeNull();
  });

  it("ignores every other key", () => {
    for (const key of ["Enter", "c", "Esc", "escape", " "]) {
      expect(escapeClaimant({ ...escape({ itemDetail: true }), key })).toBeNull();
    }
  });

  // The list is the contract every caller is a `Record` over. If a claimant is
  // added or removed, `App.tsx`'s closer map and `EscapeInput["open"]` stop
  // compiling — this asserts the list itself was a deliberate edit.
  it("is the four shell overlays, shallowest first", () => {
    expect(ESCAPE_CLAIMANTS).toEqual(["search", "capture", "navSheet", "itemDetail"]);
  });
});
