import { describe, expect, it } from "vitest";
import { toggleCalendarId, unavailableSelectedIds } from "./selection";

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

describe("unavailableSelectedIds", () => {
  const listed = [
    { id: "primary", summary: "john@twinion.net" },
    { id: "team@twinion.net", summary: "Team" },
  ];

  it("names a selected calendar the listing no longer offers", () => {
    // The poison case: polling is all-or-nothing, so this id's 403/404
    // aborts the whole snapshot on every trigger. Naming it is what gives
    // the user something to uncheck.
    expect(unavailableSelectedIds(["primary", "deleted-cal"], listed)).toEqual([
      "deleted-cal",
    ]);
  });

  it("names nothing when every selected calendar was listed", () => {
    expect(unavailableSelectedIds(["primary", "team@twinion.net"], listed)).toEqual([]);
  });

  it("names nothing when no listing has landed yet", () => {
    // Offline start or a held credential: "we haven't looked" must not
    // render as "none of these exist", which would flag every selected
    // calendar as unavailable on every cold start.
    expect(unavailableSelectedIds(["primary", "team@twinion.net"], [])).toEqual([]);
  });
});
