import { describe, expect, it } from "vitest";
import { demoMode, isDemoEnabled } from "./demo-mode";

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

  it("is off in the board world, which is a different world and not a kit variant", () => {
    expect(isDemoEnabled("?demo=board")).toBe(false);
  });
});

describe("demoMode", () => {
  it("is null with no query string, and for an unrelated param", () => {
    expect(demoMode("")).toBeNull();
    expect(demoMode("?demonstration=1")).toBeNull();
  });

  it("reads the board world only from the one exact spelling", () => {
    expect(demoMode("?demo=board")).toBe("board");
    expect(demoMode("?foo=1&demo=board")).toBe("board");
  });

  // The compatibility claim the whole change rests on: no link, bookmark or
  // gate invocation that worked before now means something different.
  it("leaves every other spelling of ?demo in the kit world it has always been", () => {
    expect(demoMode("?demo")).toBe("kit");
    expect(demoMode("?demo=1")).toBe("kit");
    expect(demoMode("?demo=true")).toBe("kit");
    expect(demoMode("?demo=boards")).toBe("kit");
    expect(demoMode("?foo=1&demo=1")).toBe("kit");
  });
});
