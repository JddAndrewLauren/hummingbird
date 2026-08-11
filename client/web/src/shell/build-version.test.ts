import { describe, expect, it } from "vitest";
import { computeBuildVersion, parseBase } from "./build-version";

function version(over: Partial<Parameters<typeof computeBuildVersion>[0]> = {}): string {
  return computeBuildVersion({
    baseText: "0.1.0\n",
    commitCount: 0,
    shallow: false,
    isMainBuild: true,
    ...over,
  });
}

describe("parseBase", () => {
  it("takes three integers and nothing else", () => {
    expect(parseBase("0.1.0\n")).toEqual({ major: 0, minor: 1, patch: 0 });
    expect(parseBase("  12.3.45  ")).toEqual({ major: 12, minor: 3, patch: 45 });
  });

  it("rejects rather than coerces", () => {
    for (const bad of ["0.1", "v0.1.0", "0.1.0-rc1", "", "nonsense", null]) {
      expect(parseBase(bad)).toBeNull();
    }
  });
});

describe("computeBuildVersion", () => {
  it("adds the commit count to the file's patch", () => {
    expect(version({ commitCount: 7 })).toBe("0.1.7");
  });

  it("is the file's own value at the merge that touched it", () => {
    expect(version({ commitCount: 0 })).toBe("0.1.0");
  });

  it("adds to an override's patch, never resetting to it", () => {
    expect(version({ baseText: "0.2.0", commitCount: 0 })).toBe("0.2.0");
    expect(version({ baseText: "0.2.0", commitCount: 3 })).toBe("0.2.3");
    // The patch in the file is a floor, not a reset point.
    expect(version({ baseText: "1.4.9", commitCount: 2 })).toBe("1.4.11");
  });

  it("renders +unknown on a shallow clone, never a bare number", () => {
    const shallow = version({ shallow: true, commitCount: 7 });
    expect(shallow).toBe("0.1.0+unknown");
    expect(shallow).not.toMatch(/^\d+\.\d+\.\d+$/);
  });

  it("renders +unknown when git could not count", () => {
    expect(version({ commitCount: null })).toBe("0.1.0+unknown");
  });

  it("renders +unknown for a missing or unparseable VERSION, without throwing", () => {
    expect(version({ baseText: null })).toBe("0.0.0+unknown");
    expect(version({ baseText: "not a version" })).toBe("0.0.0+unknown");
    // Not a bare 0.0.0: a build with no readable base may not look like a
    // real one.
    expect(version({ baseText: null })).not.toBe("0.0.0");
  });

  it("marks a non-main build +dev so a branch screenshot cannot read as deployed", () => {
    expect(version({ commitCount: 7, isMainBuild: false })).toBe("0.1.7+dev");
  });

  it("prefers +unknown over +dev when the number itself is not trustworthy", () => {
    expect(version({ shallow: true, isMainBuild: false })).toBe("0.1.0+unknown");
  });
});
