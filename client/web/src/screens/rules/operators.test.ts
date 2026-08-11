import { describe, expect, it } from "vitest";
import { defaultOperatorFor, legalOperators } from "./operators";

describe("legalOperators", () => {
  it("gives string eq and contains only", () => {
    expect(legalOperators("string")).toEqual(["eq", "contains"]);
  });

  it("gives string_list eq and contains only, same as string", () => {
    expect(legalOperators("string_list")).toEqual(["eq", "contains"]);
  });

  it("gives number eq, gt and lt only", () => {
    expect(legalOperators("number")).toEqual(["eq", "gt", "lt"]);
  });

  it("gives bool is only", () => {
    expect(legalOperators("bool")).toEqual(["is"]);
  });

  it("gives timestamp within_next/within_last only", () => {
    expect(legalOperators("timestamp")).toEqual(["within_next", "within_last"]);
  });

  it("gives date within_next/within_last only, same as timestamp", () => {
    expect(legalOperators("date")).toEqual(["within_next", "within_last"]);
  });

  it("gives dynamic the reachable subset, never within_next/within_last", () => {
    const ops = legalOperators("dynamic");
    expect(ops).toContain("eq");
    expect(ops).toContain("contains");
    expect(ops).toContain("gt");
    expect(ops).toContain("lt");
    expect(ops).toContain("is");
    expect(ops).not.toContain("within_next");
    expect(ops).not.toContain("within_last");
  });
});

describe("defaultOperatorFor", () => {
  it("is always the first legal operator", () => {
    for (const fieldType of ["string", "string_list", "number", "bool", "timestamp", "date", "dynamic"] as const) {
      expect(defaultOperatorFor(fieldType)).toBe(legalOperators(fieldType)[0]);
    }
  });
});
