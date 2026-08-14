import { describe, expect, it, vi } from "vitest";
import { watchForActivation, type ServiceWorkerLifecycle } from "./reload-on-activate";

// A fake lifecycle rather than a jsdom service worker: jsdom implements no
// `navigator.serviceWorker` at all, and the module is deliberately DOM-free
// so that is the whole point of the seam.
function lifecycle(controlled: boolean) {
  const listeners = new Set<() => void>();
  const seam: ServiceWorkerLifecycle = {
    hasController: () => controlled,
    onControllerChange(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
  return {
    seam,
    /** Control changing hands — this view's click, or another view's. */
    activate: () => {
      for (const listener of [...listeners]) {
        listener();
      }
    },
    listenerCount: () => listeners.size,
  };
}

describe("watchForActivation", () => {
  it("reloads a controlled page when control changes hands", () => {
    const { seam, activate } = lifecycle(true);
    const reload = vi.fn();

    watchForActivation(seam, reload);
    activate();

    expect(reload).toHaveBeenCalledTimes(1);
  });

  it("reloads for a change this view did not cause", () => {
    // The whole point: no banner was shown here, nothing was clicked here.
    // Another tab applied the update and this one converges anyway.
    const { seam, activate } = lifecycle(true);
    const reload = vi.fn();

    watchForActivation(seam, reload);
    activate();

    expect(reload).toHaveBeenCalledTimes(1);
  });

  it("reloads at most once", () => {
    const { seam, activate } = lifecycle(true);
    const reload = vi.fn();

    watchForActivation(seam, reload);
    activate();
    activate();
    activate();

    expect(reload).toHaveBeenCalledTimes(1);
  });

  it("does not reload a page that was uncontrolled when wired", () => {
    // First-ever visit: the worker claiming this page precached the shell it
    // is already running, so a reload would be a flash of the same build.
    const { seam, activate } = lifecycle(false);
    const reload = vi.fn();

    watchForActivation(seam, reload);
    activate();

    expect(reload).not.toHaveBeenCalled();
  });

  it("does not reload after dispose, and unsubscribes", () => {
    const { seam, activate, listenerCount } = lifecycle(true);
    const reload = vi.fn();

    const dispose = watchForActivation(seam, reload);
    dispose();
    activate();

    expect(reload).not.toHaveBeenCalled();
    // Asserted separately from the behaviour above: a dispose that only set
    // a flag would pass the first expectation while leaking a listener for
    // the life of the page.
    expect(listenerCount()).toBe(0);
  });
});
