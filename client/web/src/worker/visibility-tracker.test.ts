import { describe, expect, it } from "vitest";
import { VisibilityTracker } from "./visibility-tracker";

describe("VisibilityTracker", () => {
  it("is hidden by default, before any view has ever reported in", () => {
    const tracker = new VisibilityTracker<string>();
    expect(tracker.isHidden()).toBe(true);
  });

  it("is hidden once its one known port reports hidden", () => {
    const tracker = new VisibilityTracker<string>();
    tracker.setHidden("tab-1", true);
    expect(tracker.isHidden()).toBe(true);
  });

  it("is not hidden once its one known port reports visible", () => {
    const tracker = new VisibilityTracker<string>();
    tracker.setHidden("tab-1", false);
    expect(tracker.isHidden()).toBe(false);
  });

  it("one visible tab keeps the shared cycle running regardless of how many siblings are hidden", () => {
    const tracker = new VisibilityTracker<string>();
    tracker.setHidden("tab-1", true);
    tracker.setHidden("tab-2", true);
    tracker.setHidden("tab-3", false);
    tracker.setHidden("tab-4", true);

    expect(tracker.isHidden()).toBe(false);
  });

  it("N hidden tabs never individually re-enable it — this is one shared decision, not per tab", () => {
    const tracker = new VisibilityTracker<string>();
    for (let i = 0; i < 10; i += 1) {
      tracker.setHidden(`tab-${i}`, true);
    }
    expect(tracker.isHidden()).toBe(true);
  });

  it("a port's later report overwrites its earlier one", () => {
    const tracker = new VisibilityTracker<string>();
    tracker.setHidden("tab-1", false);
    expect(tracker.isHidden()).toBe(false);

    tracker.setHidden("tab-1", true);
    expect(tracker.isHidden()).toBe(true);
  });

  it("works with object identity as the port key, not just primitives", () => {
    const tracker = new VisibilityTracker<object>();
    const portA = {};
    const portB = {};
    tracker.setHidden(portA, true);
    tracker.setHidden(portB, false);

    expect(tracker.isHidden()).toBe(false);
  });
});
