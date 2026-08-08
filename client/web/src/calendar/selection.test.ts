import { describe, expect, it } from "vitest";
import { toggleCalendarId } from "./selection";

describe("toggleCalendarId", () => {
  it("adds an id that isn't selected yet", () => {
    expect(toggleCalendarId(["a"], "b")).toEqual(["a", "b"]);
  });

  it("removes an id that is already selected", () => {
    expect(toggleCalendarId(["a", "b"], "a")).toEqual(["b"]);
  });

  it("starts from empty", () => {
    expect(toggleCalendarId([], "a")).toEqual(["a"]);
  });
});
