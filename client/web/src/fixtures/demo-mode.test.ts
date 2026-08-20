import { describe, expect, it } from "vitest";
import { demoMode, isDemoEnabled } from "./demo-mode";

describe("isDemoEnabled", () => {
  it("is off with no query string", () => {
    expect(isDemoEnabled("")).toBe(false);
  });

  it("is off for a bare ?demo — that now means the board world, not the kit", () => {
    expect(isDemoEnabled("?demo")).toBe(false);
  });

  it("is on for ?demo=kit, alone or alongside other params", () => {
    expect(isDemoEnabled("?demo=kit")).toBe(true);
    expect(isDemoEnabled("?foo=1&demo=kit")).toBe(true);
  });

  it("is off for an unrelated param", () => {
    expect(isDemoEnabled("?demonstration=1")).toBe(false);
  });

  it("is off in the board world, which is a different world and not a kit variant", () => {
    expect(isDemoEnabled("?demo=board")).toBe(false);
    expect(isDemoEnabled("?demo=1")).toBe(false);
  });
});

describe("demoMode", () => {
  it("is null with no query string, and for an unrelated param", () => {
    expect(demoMode("")).toBeNull();
    expect(demoMode("?demonstration=1")).toBeNull();
  });

  it("reads the kit world only from the one exact spelling", () => {
    expect(demoMode("?demo=kit")).toBe("kit");
    expect(demoMode("?foo=1&demo=kit")).toBe("kit");
  });

  // The flip this issue makes (#455): every spelling that is not the kit's
  // one exact spelling is now the board world, including the bare flag that
  // used to mean the kit — no third arm and no fallback to a design-only
  // world by accident.
  it("resolves every other spelling of ?demo to the board world", () => {
    expect(demoMode("?demo")).toBe("board");
    expect(demoMode("?demo=1")).toBe("board");
    expect(demoMode("?demo=true")).toBe("board");
    expect(demoMode("?demo=board")).toBe("board");
    expect(demoMode("?foo=1&demo=1")).toBe("board");
  });
});
