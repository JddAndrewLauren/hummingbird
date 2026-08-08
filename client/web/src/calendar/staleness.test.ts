import { describe, expect, it } from "vitest";
import { formatAsOf, isStale, STALE_AFTER_MS } from "./staleness";

describe("isStale", () => {
  it("is not stale right at the as-of instant", () => {
    expect(isStale(1_000, 1_000)).toBe(false);
  });

  it("is not stale just under the threshold", () => {
    expect(isStale(0, STALE_AFTER_MS)).toBe(false);
  });

  it("is stale just over the threshold", () => {
    expect(isStale(0, STALE_AFTER_MS + 1)).toBe(true);
  });
});

describe("formatAsOf", () => {
  it("reads 'just now' under a minute old", () => {
    expect(formatAsOf(1_000, 1_000 + 30_000)).toBe("just now");
  });

  it("reads minutes ago under an hour old", () => {
    expect(formatAsOf(0, 12 * 60_000)).toBe("12m ago");
  });

  it("reads hours ago at an hour or beyond", () => {
    expect(formatAsOf(0, 3 * 60 * 60_000)).toBe("3h ago");
  });
});
