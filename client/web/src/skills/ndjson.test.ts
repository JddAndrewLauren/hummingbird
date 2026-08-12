import { describe, expect, it } from "vitest";
import { takeLines } from "./ndjson";

describe("takeLines", () => {
  it("splits complete lines and keeps the partial tail", () => {
    expect(takeLines("", 'a\nb\nc')).toEqual({ lines: ["a", "b"], rest: "c" });
  });

  it("carries one line across three chunks", () => {
    let rest = "";
    const all: string[] = [];
    for (const chunk of ['{"ty', 'pe":"progress","message":"x"', "}\n"]) {
      const taken = takeLines(rest, chunk);
      rest = taken.rest;
      all.push(...taken.lines);
    }
    expect(all).toEqual(['{"type":"progress","message":"x"}']);
    expect(rest).toBe("");
  });

  it("a chunk ending exactly on a newline leaves no tail", () => {
    expect(takeLines("", "a\n")).toEqual({ lines: ["a"], rest: "" });
  });

  it("strips a CRLF's carriage return", () => {
    expect(takeLines("", "a\r\nb\r\n")).toEqual({ lines: ["a", "b"], rest: "" });
  });

  it("a chunk with no newline at all is all tail", () => {
    expect(takeLines("", "no newline here")).toEqual({ lines: [], rest: "no newline here" });
  });

  it("drops empty lines — a blank line in NDJSON carries nothing", () => {
    expect(takeLines("", "a\n\n\nb\n")).toEqual({ lines: ["a", "b"], rest: "" });
  });
});
