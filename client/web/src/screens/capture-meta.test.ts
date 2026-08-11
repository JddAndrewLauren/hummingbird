import { describe, expect, it } from "vitest";
import { EMPTY_CAPTURE_META, resolveCaptureFields } from "./capture-meta";

describe("resolveCaptureFields", () => {
  it("leaves all three absent when the meta is at its resting state", () => {
    expect(resolveCaptureFields(EMPTY_CAPTURE_META)).toEqual({
      size: null,
      energy: null,
      context: null,
    });
  });

  it("resolves every size slider stop to the domain's own wire name", () => {
    expect(resolveCaptureFields({ energy: null, size: 0, context: "" }).size).toBe("quick");
    expect(resolveCaptureFields({ energy: null, size: 1, context: "" }).size).toBe("short");
    expect(resolveCaptureFields({ energy: null, size: 2, context: "" }).size).toBe("deep");
  });

  it("resolves every energy slider stop to the domain's own wire name", () => {
    expect(resolveCaptureFields({ energy: 0, size: null, context: "" }).energy).toBe("low");
    expect(resolveCaptureFields({ energy: 1, size: null, context: "" }).energy).toBe("medium");
    expect(resolveCaptureFields({ energy: 2, size: null, context: "" }).energy).toBe("high");
  });

  it("carries a chosen context through, and maps an empty one to null", () => {
    expect(resolveCaptureFields({ energy: null, size: null, context: "@home" }).context).toBe(
      "@home",
    );
    expect(resolveCaptureFields({ energy: null, size: null, context: "" }).context).toBeNull();
  });

  it("sets only the one field a caller touched, leaving the other two absent", () => {
    expect(resolveCaptureFields({ energy: 2, size: null, context: "" })).toEqual({
      size: null,
      energy: "high",
      context: null,
    });
  });
});
