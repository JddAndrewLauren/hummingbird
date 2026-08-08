import { describe, expect, it, vi } from "vitest";
import { coreStore, createCoreStore } from "./store";

describe("createCoreStore", () => {
  it("starts in the loading state with no api version and no error", () => {
    const store = createCoreStore();

    expect(store.getSnapshot()).toEqual({
      status: "loading",
      apiVersion: null,
      error: null,
    });
  });

  it("notifies subscribers when state changes", () => {
    const store = createCoreStore();
    const listener = vi.fn();
    store.subscribe(listener);

    store.setState({ status: "ready", apiVersion: 1 });

    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("merges partial patches into the existing snapshot", () => {
    const store = createCoreStore();

    store.setState({ status: "ready", apiVersion: 1 });

    expect(store.getSnapshot()).toEqual({
      status: "ready",
      apiVersion: 1,
      error: null,
    });
  });

  it("stops notifying a listener once it unsubscribes", () => {
    const store = createCoreStore();
    const listener = vi.fn();
    const unsubscribe = store.subscribe(listener);
    unsubscribe();

    store.setState({ status: "error", error: "boom" });

    expect(listener).not.toHaveBeenCalled();
  });

  it("exposes a stable, module-level subscribe reference on the singleton store", () => {
    // useSyncExternalStore requires a stable `subscribe` function identity
    // across renders, or React will resubscribe (and can loop) on every
    // render. `coreStore` is the one module-level singleton useStore reads.
    const first = coreStore.subscribe;
    const second = coreStore.subscribe;
    expect(first).toBe(second);
  });
});
