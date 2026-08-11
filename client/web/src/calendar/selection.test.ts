import { describe, expect, it } from "vitest";
import type { BindingDTO } from "../store/protocol";
import {
  acceptSelectionChange,
  effectiveCalendarIds,
  effectiveSelection,
  toggleCalendarId,
  tripsCalendarId,
  unavailableSelectedIds,
} from "./selection";

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

function binding(value: BindingDTO["value"]): BindingDTO[] {
  return [{ key: "trips-calendar", known: true, pending: false, value }];
}

describe("tripsCalendarId", () => {
  it("is null while the bindings table has not answered", () => {
    expect(tripsCalendarId(null)).toBeNull();
  });

  it("is null for an unset row, a non-text row and a blanked one", () => {
    expect(tripsCalendarId(binding({ state: "unset" }))).toBeNull();
    expect(tripsCalendarId(binding({ state: "other", raw: "7" }))).toBeNull();
    expect(tripsCalendarId(binding({ state: "text", text: "   " }))).toBeNull();
  });

  it("is the trimmed calendar id for a text row", () => {
    expect(tripsCalendarId(binding({ state: "text", text: " trips@g " }))).toBe("trips@g");
  });
});

describe("effectiveSelection", () => {
  it("polls every ticked calendar on the standard horizon when nothing is bound", () => {
    expect(effectiveSelection(["a", "b"], null)).toEqual([
      { id: "a", horizon: "standard" },
      { id: "b", horizon: "standard" },
    ]);
  });

  it("adds the bound trips calendar nobody ticked, on the long horizon", () => {
    expect(effectiveSelection(["a"], "trips@g")).toEqual([
      { id: "a", horizon: "standard" },
      { id: "trips@g", horizon: "long" },
    ]);
  });

  it("does not list a ticked trips calendar twice, and gives it the long horizon", () => {
    expect(effectiveSelection(["a", "trips@g"], "trips@g")).toEqual([
      { id: "a", horizon: "standard" },
      { id: "trips@g", horizon: "long" },
    ]);
  });

  it("re-computes cleanly when the binding moves to another calendar", () => {
    // Deriving rather than persisting is what makes this true: the old
    // calendar simply stops being polled, with nothing left behind that
    // would keep polling it forever with no on-screen reason why.
    expect(effectiveCalendarIds(["a"], "old@g")).toEqual(["a", "old@g"]);
    expect(effectiveCalendarIds(["a"], "new@g")).toEqual(["a", "new@g"]);
  });
});

describe("acceptSelectionChange", () => {
  it("refuses a change that unticks the bound trips calendar", () => {
    expect(acceptSelectionChange(["a"], "trips@g")).toBeNull();
  });

  it("accepts an ordinary change and never persists the derived id", () => {
    expect(acceptSelectionChange(["a", "b", "trips@g"], "trips@g")).toEqual(["a", "b"]);
  });

  it("accepts everything while nothing is bound", () => {
    expect(acceptSelectionChange([], null)).toEqual([]);
    expect(acceptSelectionChange(["a"], null)).toEqual(["a"]);
  });
});
