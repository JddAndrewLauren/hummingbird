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

  it("checks for an update on window focus, gated by the gap", () => {
    const check = vi.fn();
    render(<Probe signal={createAppUpdateSignal()} check={check} />);

    fireEvent.focus(window);
    fireEvent.focus(window);
    fireEvent.focus(window);
    // The first focus checks; the rest land inside `MIN_CHECK_GAP_MS`.
    expect(check).toHaveBeenCalledTimes(1);
    expect(MIN_CHECK_GAP_MS).toBeGreaterThan(0);
  });

  // The two below are one regression, from two sides: a rerender used to
  // rebuild the checker and restart the interval, which reset the gap and
  // meant the hourly tick could never reach an hour. `App` rerenders every
  // 30 seconds for `useSyncWiring.ts`'s status clock, so in the real app
  // both schedules were dead.
  it("holds the gap across a rerender", () => {
    const check = vi.fn();
    const signal = createAppUpdateSignal();
    const view = render(<Probe signal={signal} check={check} />);

    fireEvent.focus(window);
    view.rerender(<Probe signal={signal} check={check} />);
    fireEvent.focus(window);

    // The second focus is still inside `MIN_CHECK_GAP_MS`; the rerender
    // between them must not have made it a first check again.
    expect(check).toHaveBeenCalledTimes(1);
  });

  it("fires the hourly check while rerendering far faster than the interval", () => {
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

      expect(check).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });

  it("stops listening once unmounted", () => {
    const check = vi.fn();
    const view = render(<Probe signal={createAppUpdateSignal()} check={check} />);
    view.unmount();
    fireEvent.focus(window);
    expect(check).not.toHaveBeenCalled();
  });
});
