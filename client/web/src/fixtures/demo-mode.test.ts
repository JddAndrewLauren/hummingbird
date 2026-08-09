import { describe, expect, it } from "vitest";
import { isDemoEnabled } from "./demo-mode";

describe("isDemoEnabled", () => {
  it("is off with no query string", () => {
    expect(isDemoEnabled("")).toBe(false);
  });

  it("is on for a bare ?demo", () => {
    expect(isDemoEnabled("?demo")).toBe(true);
  });

  it("is on for ?demo alongside other params", () => {
    expect(isDemoEnabled("?foo=1&demo=1")).toBe(true);
  });

  it("is off for an unrelated param", () => {
    expect(isDemoEnabled("?demonstration=1")).toBe(false);
  });
});
