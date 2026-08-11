import { describe, expect, it } from "vitest";
import { coreInstanceLabel, coreStatusLabel } from "./status-label";

describe("coreStatusLabel", () => {
  it("reports a ready core with its api version", () => {
    expect(coreStatusLabel("ready", 1)).toBe("core ready · api v1");
  });

  it("omits the version when a ready core reported none", () => {
    expect(coreStatusLabel("ready", null)).toBe("core ready");
  });

  it("still shows api v0, so a falsy check can never be mistaken for no version", () => {
    expect(coreStatusLabel("ready", 0)).toBe("core ready · api v0");
  });

  it("reports loading", () => {
    expect(coreStatusLabel("loading", null)).toBe("starting core…");
  });

  it("reports failure without reassurance", () => {
    expect(coreStatusLabel("error", null)).toBe("core failed");
  });
});

// #172: what a person reads in two windows to settle ADR-0010's assumption.
describe("coreInstanceLabel", () => {
  it("names the core instance and this view's ordinal", () => {
    expect(coreInstanceLabel("3f2a1b8c", 2)).toBe("Core instance 3f2a1b8c · this view #2.");
  });

  it("says nothing at all until both halves are known", () => {
    // A half-sentence naming an instance it cannot identify is worse than
    // no line — the reader is comparing two windows' ids by eye.
    expect(coreInstanceLabel(null, 2)).toBeNull();
    expect(coreInstanceLabel("3f2a1b8c", null)).toBeNull();
    expect(coreInstanceLabel(null, null)).toBeNull();
  });

  it("renders view #1 as a real ordinal, never suppressed as falsy", () => {
    expect(coreInstanceLabel("3f2a1b8c", 1)).toBe("Core instance 3f2a1b8c · this view #1.");
  });
});
