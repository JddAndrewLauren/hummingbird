import { describe, expect, it, vi } from "vitest";
import { createUpdateChecker, MIN_CHECK_GAP_MS } from "./update-check";

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

  it("collapses a burst of focus requests into exactly one check", () => {
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
});
