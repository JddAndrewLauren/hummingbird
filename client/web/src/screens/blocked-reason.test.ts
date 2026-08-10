import { describe, expect, it } from "vitest";
import { blockedReasonLabel } from "./blocked-reason";

describe("blockedReasonLabel", () => {
  it("names the single blocker by title", () => {
    expect(blockedReasonLabel(["Ship the release"])).toBe("Blocked by: Ship the release");
  });

  it("joins two blockers with 'and'", () => {
    expect(blockedReasonLabel(["A", "B"])).toBe("Blocked by: A and B");
  });

  it("joins three or more with commas and a trailing 'and'", () => {
    expect(blockedReasonLabel(["A", "B", "C"])).toBe("Blocked by: A, B and C");
  });

  it("reads honestly when the blocker list is somehow empty", () => {
    expect(blockedReasonLabel([])).toBe("Blocked");
  });
});
