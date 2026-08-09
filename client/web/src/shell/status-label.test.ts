import { describe, expect, it } from "vitest";
import { coreStatusLabel } from "./status-label";

describe("coreStatusLabel", () => {
  it("reports a ready core with its api version", () => {
    expect(coreStatusLabel("ready", 1)).toBe("core ready · api v1");
  });

  it("omits the version when a ready core reported none", () => {
    expect(coreStatusLabel("ready", null)).toBe("core ready");
  });

  it("still shows api v0, so a falsy check can never be mistaken for no version", () => {
    expect(coreStatusLabel("ready", 0)).toBe("core ready · api v0");
  });

  it("reports loading", () => {
    expect(coreStatusLabel("loading", null)).toBe("starting core…");
  });

  it("reports failure without reassurance", () => {
    expect(coreStatusLabel("error", null)).toBe("core failed");
  });
});
