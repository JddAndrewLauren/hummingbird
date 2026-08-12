import { describe, expect, it } from "vitest";
import { classifyLine, microtaskResult } from "./envelope";

describe("classifyLine", () => {
  it("reads a progress line", () => {
    expect(classifyLine('{"type":"progress","message":"running skill microtask"}')).toEqual({
      kind: "progress",
      message: "running skill microtask",
    });
  });

  it("reads a stamped ok line", () => {
    expect(
      classifyLine('{"ok":true,"skill":"microtask","result":{"steps":[],"note":"n"},"backend":"anthropic","model":"opus"}'),
    ).toEqual({
      kind: "ok",
      result: { steps: [], note: "n" },
      backend: "anthropic",
      model: "opus",
    });
  });

  it("reads a stamped failure line", () => {
    expect(classifyLine('{"ok":false,"skill":"microtask","error":"nope","backend":"anthropic","model":null}')).toEqual({
      kind: "failed",
      error: "nope",
      backend: "anthropic",
      model: null,
    });
  });

  /** An unstamped line means nothing was attempted (ADR-0018). `null` is
   * the only honest reading, and it must never be replaced with a literal
   * — that is what keeps #273's "not hardcoded at the render site" true by
   * construction rather than by review. */
  it("an unstamped line carries nulls, never a default name", () => {
    const line = classifyLine('{"ok":false,"skill":null,"error":"Cloud runner unreachable."}');
    expect(line).toEqual({
      kind: "failed",
      error: "Cloud runner unreachable.",
      backend: null,
      model: null,
    });
  });

  it("a non-string backend or model reads as absent", () => {
    expect(classifyLine('{"ok":true,"result":null,"backend":42,"model":{"id":"x"}}')).toMatchObject({
      backend: null,
      model: null,
    });
  });

  /** Truthy-coercing this would render a failed run as a finished one. */
  it('a string "true" for ok is unreadable, not a success', () => {
    expect(classifyLine('{"ok":"true","result":{}}')).toEqual({ kind: "unreadable" });
    expect(classifyLine('{"ok":1,"result":{}}')).toEqual({ kind: "unreadable" });
  });

  it("garbage, a bare scalar, an array and a null are all unreadable", () => {
    for (const text of ["not json", "42", '"a string"', "[1,2]", "null"]) {
      expect(classifyLine(text), text).toEqual({ kind: "unreadable" });
    }
  });

  it("a progress line with no message string is unreadable", () => {
    expect(classifyLine('{"type":"progress"}')).toEqual({ kind: "unreadable" });
  });

  it("an ok:false line with no error string is unreadable", () => {
    expect(classifyLine('{"ok":false,"skill":"microtask"}')).toEqual({ kind: "unreadable" });
  });
});

describe("microtaskResult", () => {
  it("reads the schema's two fields", () => {
    expect(microtaskResult({ steps: ["put on music"], note: "kept 2" })).toEqual({
      steps: ["put on music"],
      note: "kept 2",
    });
  });

  it("is null for anything that is not the schema's shape", () => {
    for (const value of [null, undefined, 42, "steps", [], {}, { steps: [], note: "" }]) {
      expect(microtaskResult(value)).toBeNull();
    }
  });

  it("drops non-string entries rather than refusing the whole result", () => {
    expect(microtaskResult({ steps: ["a", 2, null, "b"], note: "" })).toEqual({
      steps: ["a", "b"],
      note: "",
    });
  });
});
