// Maps the store's calendar slice onto the design system's `ContextTile`
// props (issue #73's tile, restyled onto the brand tokens). The old
// hand-rolled tile computed all of this inline while it rendered; pulling
// it out keeps the decisions — which are the honest ones — in a unit-tested
// pure module, and leaves the component a presentational shell.

import type { CalendarState } from "../store/store";
import { formatAsOf, isStale } from "./staleness";

/** The props subset this adapter produces. Structurally a
 * `ContextTileProps` (components/domain/ContextTile), named separately so
 * this module stays free of any React import. */
export interface CalendarTileProps {
  kind: "no_snapshot" | "none" | "in_progress" | "upcoming";
  title?: string;
  timeLabel?: string;
  href?: string;
  asOf?: string;
  stale: boolean;
}

function formatEventTime(ms: number): string {
  return new Date(ms).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

/** `null` on a never-opted-in device: no tile, no signal, nothing for a
 * ranking consumer to lean on (the Agent Brief's "opt-in is per-device...
 * unconstrained ranking" criterion). */
export function contextTileProps(
  calendar: CalendarState,
  nowMs: number,
): CalendarTileProps | null {
  if (!calendar.connected) {
    return null;
  }

  const { tileKind, tileEvent, asOfMs } = calendar;
  const stale = asOfMs !== null && isStale(asOfMs, nowMs);
  // Present only when there is a real moment to report. `no_snapshot` is no
  // data at all and has no `asOfMs`, which is exactly what distinguishes it
  // from `none` — a real snapshot reporting a genuinely clear calendar as of
  // a known moment. Offline that difference is the whole signal: an empty
  // day read from a two-day-old snapshot is not an empty day.
  const asOf = asOfMs !== null ? formatAsOf(asOfMs, nowMs) : undefined;

  if (tileKind === "no_snapshot" || tileKind === "none" || !tileEvent) {
    return { kind: tileKind === "in_progress" || tileKind === "upcoming" ? "none" : tileKind, asOf, stale };
  }

  return {
    kind: tileKind,
    title: tileEvent.title,
    timeLabel: tileEvent.allDay
      ? "All day"
      : `${formatEventTime(tileEvent.startMs)}–${formatEventTime(tileEvent.endMs)}`,
    href: tileEvent.htmlLink ?? undefined,
    asOf,
    stale,
  };
}
