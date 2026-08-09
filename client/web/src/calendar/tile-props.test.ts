import { describe, expect, it } from "vitest";
import type { CalendarState } from "../store/store";
import { STALE_AFTER_MS } from "./staleness";
import { contextTileProps } from "./tile-props";

const NOW = 10 * 60 * 60 * 1000;

function state(patch: Partial<CalendarState> = {}): CalendarState {
  return {
    connected: true,
    needsReconnect: false,
    selectedCalendarIds: [],
    availableCalendars: [],
    lastPollOutcome: null,
    tileKind: "no_snapshot",
    tileEvent: null,
    asOfMs: null,
    ...patch,
  };
}

describe("contextTileProps", () => {
  it("renders no tile on a never-opted-in device", () => {
    expect(contextTileProps(state({ connected: false }), NOW)).toBeNull();
  });

  it("omits the as-of line for no_snapshot, which has no moment to report", () => {
    const props = contextTileProps(state({ tileKind: "no_snapshot" }), NOW);
    expect(props).toEqual({ kind: "no_snapshot", asOf: undefined, stale: false });
  });

  it("keeps the as-of line for a real snapshot reporting a clear calendar", () => {
    const props = contextTileProps(
      state({ tileKind: "none", asOfMs: NOW - 12 * 60_000 }),
      NOW,
    );
    expect(props).toEqual({ kind: "none", asOf: "12m ago", stale: false });
  });

  it("omits the as-of line for a clear calendar with no known moment, since only that line separates it from no data at all", () => {
    const props = contextTileProps(state({ tileKind: "none", asOfMs: null }), NOW);
    expect(props).toEqual({ kind: "none", asOf: undefined, stale: false });
  });

  it("turns stale past the threshold and keeps showing the data", () => {
    const asOfMs = NOW - (STALE_AFTER_MS + 60_000);
    const props = contextTileProps(
      state({
        tileKind: "in_progress",
        asOfMs,
        tileEvent: {
          title: "Standup",
          startMs: NOW - 60_000,
          endMs: NOW + 60_000,
          allDay: false,
          htmlLink: "https://calendar.example/e/1",
        },
      }),
      NOW,
    );
    expect(props?.stale).toBe(true);
    expect(props?.title).toBe("Standup");
    expect(props?.href).toBe("https://calendar.example/e/1");
  });

  it("labels an all-day event without a time range", () => {
    const props = contextTileProps(
      state({
        tileKind: "upcoming",
        asOfMs: NOW,
        tileEvent: {
          title: "Conference",
          startMs: NOW,
          endMs: NOW + 86_400_000,
          allDay: true,
          htmlLink: null,
        },
      }),
      NOW,
    );
    expect(props?.timeLabel).toBe("All day");
    expect(props?.href).toBeUndefined();
  });

  it("labels a timed event as a range joined by an en dash, not a hyphen, per the design system's dash rule", () => {
    const startMs = NOW;
    const endMs = NOW + 45 * 60_000;
    const props = contextTileProps(
      state({
        tileKind: "in_progress",
        asOfMs: NOW,
        tileEvent: {
          title: "Design review",
          startMs,
          endMs,
          allDay: false,
          htmlLink: null,
        },
      }),
      NOW,
    );
    // The ends are formatted with `toLocaleTimeString`, whose output depends
    // on the machine's timezone and locale, so the expected text is derived
    // from the same API rather than hard-coded — what is asserted here is the
    // shape (both ends present, in order) and the separator, which are this
    // module's decisions and not the platform's.
    const at = (ms: number) =>
      new Date(ms).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
    expect(props?.timeLabel).toBe(`${at(startMs)}–${at(endMs)}`);
    expect(props?.timeLabel?.split("–")).toEqual([at(startMs), at(endMs)]);
    expect(props?.timeLabel).not.toBe(`${at(startMs)}-${at(endMs)}`);
  });

  it("renders an event-shaped kind carrying no event as an empty tile", () => {
    const props = contextTileProps(
      state({ tileKind: "upcoming", tileEvent: null, asOfMs: NOW }),
      NOW,
    );
    expect(props?.kind).toBe("none");
  });
});
