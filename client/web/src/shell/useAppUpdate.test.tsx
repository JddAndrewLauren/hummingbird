// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen } from "../test/component";
import { createAppUpdateSignal, type AppUpdateSignal } from "./app-update";
import { UpdateBanner } from "./UpdateBanner";
import { useAppUpdate } from "./useAppUpdate";
import { MIN_CHECK_GAP_MS, UPDATE_CHECK_INTERVAL_MS } from "./update-check";

// Mounted rather than called, for `useLedgerWiring.test.tsx`'s own reason:
// a hook that is exported, unit-tested and never wired compiles clean and
// does nothing. This is the signal -> hook -> strip path end to end, which
// is the wiring `App.tsx` does and nothing else can prove.

// The `check` handed down is deliberately a FRESH function identity on every
// render, because that is what the real caller does: `App.tsx` calls
// `useAppUpdate()` with no arguments at all, and a default parameter is
// re-evaluated per render. A probe passing a hoisted callback would let a
// hook that rebuilt its schedules on identity pass every test here and still
// never fire an hourly check in the app.
function Probe({ signal, check }: { signal: AppUpdateSignal; check?: () => void }) {
  const { ready, onReload } = useAppUpdate(signal, () => check?.());
  return ready ? <UpdateBanner onReload={onReload} /> : null;
}

// Matched loosely on purpose: these tests are about the signal -> hook ->
// strip path, and the exact wording (including the origin-wide scope
// sentence) is `UpdateBanner.test.tsx`'s own subject.
const COPY = /A new version of hummingbird is ready/;

// `visibilityState` is an accessor on `Document.prototype`, so `vi.spyOn` on
// the instance fails outright. Defining an own property on `document`
// shadows it; `delete` restores jsdom's, and it MUST run — a leaked "hidden"
// would silently change every later test in the file.
function withVisibilityState(state: DocumentVisibilityState, body: () => void) {
  Object.defineProperty(document, "visibilityState", {
    configurable: true,
    get: () => state,
  });
  try {
    body();
  } finally {
    // `Reflect.deleteProperty` rather than `delete`: `visibilityState` is
    // declared readonly, so the operator does not typecheck against it.
    Reflect.deleteProperty(document, "visibilityState");
  }
}

// jsdom has no `PageTransitionEvent` constructor, and the hook deliberately
// reads nothing off the event (not even `persisted`), so a bare `Event`
// carrying the right type is the whole signal.
function firePageShow() {
  window.dispatchEvent(new Event("pageshow"));
}

describe("useAppUpdate", () => {
  it("shows nothing until a worker is waiting", () => {
    render(<Probe signal={createAppUpdateSignal()} />);
    expect(screen.queryByText(COPY)).toBeNull();
  });

  it("shows the strip once the signal is marked ready, and reloads on click", () => {
    const signal = createAppUpdateSignal();
    const apply = vi.fn();
    render(<Probe signal={signal} />);

    act(() => {
      signal.markReady(apply);
    });

    expect(screen.getByText(COPY)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Reload" }));
    expect(apply).toHaveBeenCalledTimes(1);
  });

  it("checks on mount, before any signal arrives", () => {
    const check = vi.fn();
    render(<Probe signal={createAppUpdateSignal()} check={check} />);
    // A view opened across a deploy must not have to wait to be resumed.
    expect(check).toHaveBeenCalledTimes(1);
  });

  it("collapses a resume's several signals into one check", () => {
    const check = vi.fn();
    render(<Probe signal={createAppUpdateSignal()} check={check} />);

    // A real resume fires these together; the mount check is already one of
    // them. All five signals share one checker precisely so this is 1.
    firePageShow();
    withVisibilityState("visible", () => {
      fireEvent(document, new Event("visibilitychange"));
    });
    fireEvent.focus(window);

    expect(check).toHaveBeenCalledTimes(1);
    expect(MIN_CHECK_GAP_MS).toBeGreaterThan(0);
  });

  it("checks again on each signal once the gap has elapsed", () => {
    vi.useFakeTimers();
    try {
      const check = vi.fn();
      render(<Probe signal={createAppUpdateSignal()} check={check} />);
      expect(check).toHaveBeenCalledTimes(1);

      // Each signal is proved to reach the checker on its own — the point of
      // the phase is that a device stuck on one of them still discovers a
      // deploy. Stepping past the gap between them keeps this about the
      // listeners rather than about the gap, which has its own test.
      vi.advanceTimersByTime(MIN_CHECK_GAP_MS);
      fireEvent.focus(window);
      expect(check).toHaveBeenCalledTimes(2);

      vi.advanceTimersByTime(MIN_CHECK_GAP_MS);
      withVisibilityState("visible", () => {
        fireEvent(document, new Event("visibilitychange"));
      });
      expect(check).toHaveBeenCalledTimes(3);

      vi.advanceTimersByTime(MIN_CHECK_GAP_MS);
      firePageShow();
      expect(check).toHaveBeenCalledTimes(4);
    } finally {
      vi.useRealTimers();
    }
  });

  it("ignores a visibilitychange that reports hidden", () => {
    vi.useFakeTimers();
    try {
      const check = vi.fn();
      render(<Probe signal={createAppUpdateSignal()} check={check} />);
      vi.advanceTimersByTime(MIN_CHECK_GAP_MS);

      withVisibilityState("hidden", () => {
        fireEvent(document, new Event("visibilitychange"));
      });

      // The gap has elapsed, so a check here would be let through — only the
      // `visibilityState` guard stops it. Without this the test would pass
      // against a hook that asked the origin every time a view was hidden.
      expect(check).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });

  // The two below are one regression, from two sides: a rerender used to
  // rebuild the checker and restart the interval, which reset the gap and
  // meant the hourly tick could never reach an hour. `App` rerenders every
  // 30 seconds for `useSyncWiring.ts`'s status clock, so in the real app
  // both schedules were dead.
  it("holds the gap across a rerender", () => {
    vi.useFakeTimers();
    try {
      const check = vi.fn();
      const signal = createAppUpdateSignal();
      const view = render(<Probe signal={signal} check={check} />);

      // Step past the gap first, so the focus below is a real check rather
      // than one the mount check would have swallowed anyway — otherwise
      // this passes against a hook that rebuilds its checker per render.
      vi.advanceTimersByTime(MIN_CHECK_GAP_MS);
      fireEvent.focus(window);
      view.rerender(<Probe signal={signal} check={check} />);
      fireEvent.focus(window);

      // The second focus is still inside `MIN_CHECK_GAP_MS`; the rerender
      // between them must not have made it a first check again.
      expect(check).toHaveBeenCalledTimes(2);
    } finally {
      vi.useRealTimers();
    }
  });

  it("fires the periodic check while rerendering far faster than the interval", () => {
    vi.useFakeTimers();
    try {
      const check = vi.fn();
      const signal = createAppUpdateSignal();
      const view = render(<Probe signal={signal} check={check} />);

      // `useSyncWiring.ts`'s `STATUS_CLOCK_TICK_MS`, run past one full hour.
      const TICK_MS = 30 * 1000;
      for (let elapsed = 0; elapsed <= UPDATE_CHECK_INTERVAL_MS; elapsed += TICK_MS) {
        act(() => {
          vi.advanceTimersByTime(TICK_MS);
        });
        view.rerender(<Probe signal={signal} check={check} />);
      }

      // One on mount, one when the interval comes round — an interval
      // restarted by any of those rerenders would never have reached it.
      expect(check).toHaveBeenCalledTimes(2);
    } finally {
      vi.useRealTimers();
    }
  });

  it("stops listening on every signal once unmounted", () => {
    vi.useFakeTimers();
    try {
      const check = vi.fn();
      const view = render(<Probe signal={createAppUpdateSignal()} check={check} />);
      view.unmount();
      check.mockClear();

      // Past the gap, so nothing here is being swallowed by the rate limiter
      // rather than by the cleanup that is actually under test.
      vi.advanceTimersByTime(MIN_CHECK_GAP_MS);
      fireEvent.focus(window);
      withVisibilityState("visible", () => {
        fireEvent(document, new Event("visibilitychange"));
      });
      firePageShow();
      vi.advanceTimersByTime(UPDATE_CHECK_INTERVAL_MS);

      expect(check).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });
});
