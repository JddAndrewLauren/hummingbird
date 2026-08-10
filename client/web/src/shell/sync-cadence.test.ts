import { describe, expect, it, vi } from "vitest";
import { createSyncCadence, shouldRunTimerTick, toCoreTrigger } from "./sync-cadence";

describe("shouldRunTimerTick", () => {
  it("runs when visible and online", () => {
    expect(shouldRunTimerTick(false, true)).toBe(true);
  });

  it("is paused while the document is hidden, even if online", () => {
    expect(shouldRunTimerTick(true, true)).toBe(false);
  });

  it("is paused while offline, even if visible", () => {
    expect(shouldRunTimerTick(false, false)).toBe(false);
  });

  it("is paused when both hidden and offline", () => {
    expect(shouldRunTimerTick(true, false)).toBe(false);
  });
});

describe("toCoreTrigger", () => {
  it("maps every user-facing trigger to \"user\" — ADR-0007 resets backoff on a user trigger", () => {
    expect(toCoreTrigger("open")).toBe("user");
    expect(toCoreTrigger("reconnect")).toBe("user");
    expect(toCoreTrigger("focus")).toBe("user");
  });

  it('maps the unattended timer to "timer"', () => {
    expect(toCoreTrigger("timer")).toBe("timer");
  });
});

describe("createSyncCadence", () => {
  it("onOpen fires exactly one cycle with a user trigger", () => {
    const run = vi.fn();
    createSyncCadence(run).onOpen();
    expect(run).toHaveBeenCalledTimes(1);
    expect(run).toHaveBeenCalledWith("user");
  });

  it("onReconnect fires exactly one cycle with a user trigger", () => {
    const run = vi.fn();
    createSyncCadence(run).onReconnect();
    expect(run).toHaveBeenCalledTimes(1);
    expect(run).toHaveBeenCalledWith("user");
  });

  it("onFocus fires exactly one cycle with a user trigger", () => {
    const run = vi.fn();
    createSyncCadence(run).onFocus();
    expect(run).toHaveBeenCalledTimes(1);
    expect(run).toHaveBeenCalledWith("user");
  });

  it("two onFocus calls fire exactly two cycles — no de-dupe hides a real repeated gesture", () => {
    const run = vi.fn();
    const cadence = createSyncCadence(run);
    cadence.onFocus();
    cadence.onFocus();
    expect(run).toHaveBeenCalledTimes(2);
  });

  it("onTimerTick fires a timer-triggered cycle when visible and online", () => {
    const run = vi.fn();
    createSyncCadence(run).onTimerTick(false, true);
    expect(run).toHaveBeenCalledTimes(1);
    expect(run).toHaveBeenCalledWith("timer");
  });

  it("onTimerTick fires nothing while hidden — the sync timer is paused, proven", () => {
    const run = vi.fn();
    createSyncCadence(run).onTimerTick(true, true);
    expect(run).not.toHaveBeenCalled();
  });

  it("onTimerTick fires nothing while offline", () => {
    const run = vi.fn();
    createSyncCadence(run).onTimerTick(false, false);
    expect(run).not.toHaveBeenCalled();
  });
});
