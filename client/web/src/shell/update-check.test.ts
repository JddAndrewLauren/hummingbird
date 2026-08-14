import { describe, expect, it, vi } from "vitest";
import {
  createUpdateChecker,
  MIN_CHECK_GAP_MS,
  UPDATE_CHECK_INTERVAL_MS,
} from "./update-check";

function checker() {
  const check = vi.fn();
  let nowMs = 1_000_000;
  const subject = createUpdateChecker(check, () => nowMs);
  return {
    check,
    request: () => subject.request(),
    advance: (ms: number) => {
      nowMs += ms;
    },
  };
}

describe("createUpdateChecker", () => {
  it("checks on the first request", () => {
    const { check, request } = checker();
    request();
    expect(check).toHaveBeenCalledTimes(1);
  });

  it("drops a request inside the gap", () => {
    const { check, request, advance } = checker();
    request();
    advance(MIN_CHECK_GAP_MS - 1);
    request();
    expect(check).toHaveBeenCalledTimes(1);
  });

  it("checks again once the gap has elapsed", () => {
    const { check, request, advance } = checker();
    request();
    advance(MIN_CHECK_GAP_MS);
    request();
    expect(check).toHaveBeenCalledTimes(2);
  });

  // Signal-agnostic on purpose: `useAppUpdate.ts` now feeds five signals
  // into one checker, and a resume fires several of them in the same
  // instant (mount + `pageshow` + `focus`, or `visibilitychange` + `focus`).
  // Naming this "focus" again would tempt the next reader to add a
  // per-signal limiter beside the gap.
  it("collapses a burst of requests from any signals into exactly one check", () => {
    const { check, request } = checker();
    for (let i = 0; i < 20; i += 1) {
      request();
    }
    expect(check).toHaveBeenCalledTimes(1);
  });

  it("never queues a dropped request — the gap elapsing alone fires nothing", () => {
    const { check, request, advance } = checker();
    request();
    request();
    advance(MIN_CHECK_GAP_MS * 3);
    expect(check).toHaveBeenCalledTimes(1);
  });

  // The two constants are set independently and read by different code, so
  // nothing but this pins their order. Inverted, the periodic tick would
  // land inside its own gap and the background cadence would silently stop
  // being a cadence at all — a failure with no symptom until a device sits
  // foregrounded across a deploy and never notices.
  it("keeps the gap shorter than the interval it rate-limits", () => {
    expect(MIN_CHECK_GAP_MS).toBeGreaterThan(0);
    expect(MIN_CHECK_GAP_MS).toBeLessThan(UPDATE_CHECK_INTERVAL_MS);
  });
});
