import { describe, expect, it, vi } from "vitest";
import { createAppUpdateSignal } from "./app-update";

describe("createAppUpdateSignal", () => {
  it("starts with nothing waiting and nothing to apply", () => {
    const signal = createAppUpdateSignal();
    expect(signal.getSnapshot()).toEqual({ ready: false, apply: null });
  });

  it("returns a reference-stable snapshot while nothing has changed", () => {
    // The whole reason a frozen object is held rather than minted per call:
    // `useSyncExternalStore` compares by reference and would re-render
    // forever against a fresh object.
    const signal = createAppUpdateSignal();
    expect(signal.getSnapshot()).toBe(signal.getSnapshot());

    signal.markReady(() => {});
    const ready = signal.getSnapshot();
    expect(ready).toBe(signal.getSnapshot());
  });

  it("notifies every subscriber and flips ready on markReady", () => {
    const signal = createAppUpdateSignal();
    const one = vi.fn();
    const two = vi.fn();
    signal.subscribe(one);
    signal.subscribe(two);

    const apply = vi.fn();
    signal.markReady(apply);

    expect(one).toHaveBeenCalledTimes(1);
    expect(two).toHaveBeenCalledTimes(1);
    expect(signal.getSnapshot().ready).toBe(true);
    signal.getSnapshot().apply?.();
    expect(apply).toHaveBeenCalledTimes(1);
  });

  it("replaces apply on a second markReady, so the button applies the freshest worker", () => {
    const signal = createAppUpdateSignal();
    const first = vi.fn();
    const second = vi.fn();
    signal.markReady(first);
    signal.markReady(second);

    signal.getSnapshot().apply?.();
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);
    expect(signal.getSnapshot().ready).toBe(true);
  });

  it("stops notifying an unsubscribed listener", () => {
    const signal = createAppUpdateSignal();
    const listener = vi.fn();
    const unsubscribe = signal.subscribe(listener);
    unsubscribe();

    signal.markReady(() => {});
    expect(listener).not.toHaveBeenCalled();
  });
});
