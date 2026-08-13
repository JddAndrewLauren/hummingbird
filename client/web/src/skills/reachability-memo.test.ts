import { describe, expect, it } from "vitest";
import { EMPTY_MEMO, isFreshDead, markDead, markReachable } from "./reachability-memo";

describe("reachability-memo", () => {
  it("an untouched memo is never fresh-dead", () => {
    expect(isFreshDead(EMPTY_MEMO, "cloud", 0)).toBe(false);
  });

  it("marking dead makes it fresh-dead immediately", () => {
    const memo = markDead(EMPTY_MEMO, "cloud", 1_000, 30_000);
    expect(isFreshDead(memo, "cloud", 1_000)).toBe(true);
  });

  it("a fresh-dead memo expires with the passed-in expiry, not a real clock", () => {
    const memo = markDead(EMPTY_MEMO, "cloud", 1_000, 30_000);
    expect(isFreshDead(memo, "cloud", 30_999)).toBe(true);
    expect(isFreshDead(memo, "cloud", 31_001)).toBe(false);
  });

  it("marking reachable is never fresh-dead", () => {
    const memo = markReachable(EMPTY_MEMO, "cloud", 1_000, 30_000);
    expect(isFreshDead(memo, "cloud", 1_000)).toBe(false);
  });

  it("marking one backend dead leaves another untouched", () => {
    const memo = markDead(EMPTY_MEMO, "cloud", 0, 30_000);
    expect(isFreshDead(memo, "home", 0)).toBe(false);
  });

  it("is pure: the input memo is never mutated", () => {
    const before = { ...EMPTY_MEMO };
    markDead(EMPTY_MEMO, "cloud", 0, 30_000);
    expect(EMPTY_MEMO).toEqual(before);
  });
});
