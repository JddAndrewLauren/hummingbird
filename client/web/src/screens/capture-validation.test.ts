import { describe, expect, it } from "vitest";
import { canSubmitCapture } from "./capture-validation";

describe("canSubmitCapture", () => {
  it("refuses an empty string", () => {
    expect(canSubmitCapture("")).toBe(false);
  });

  it("refuses a whitespace-only draft — a junk row must never wedge the queue", () => {
    expect(canSubmitCapture("   ")).toBe(false);
    expect(canSubmitCapture("\t\n  ")).toBe(false);
  });

  it("accepts real text, padding included", () => {
    expect(canSubmitCapture("buy milk")).toBe(true);
    expect(canSubmitCapture("  buy milk  ")).toBe(true);
  });
});
