import type { CalendarState } from "../store/store";
import { formatAsOf, isStale } from "./staleness";

// The current/next context tile (issue #73). Renders nothing on a
// never-opted-in device (`connected: false`) — the Agent Brief's "opt-in is
// per-device... unconstrained ranking" criterion starts here: no tile, no
// signal, nothing for a ranking consumer to lean on. Links out via a plain
// `<a>` and never mints or touches a Linear Action (ADR-0002 rule 1 applies
// transitively: this is read-only calendar context, same as #70's mirror).

export interface ContextTileProps {
  calendar: CalendarState;
  nowMs: number;
}

function formatEventTime(ms: number): string {
  return new Date(ms).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

export function ContextTile({ calendar, nowMs }: ContextTileProps) {
  if (!calendar.connected) {
    return null;
  }

  const { tileKind, tileEvent, asOfMs } = calendar;

  if (tileKind === "no_snapshot" || tileKind === "none" || !tileEvent) {
    return (
      <div
        data-testid="context-tile"
        className="rounded-lg border border-slate-800 p-4 text-sm text-slate-400"
      >
        No current or upcoming event.
      </div>
    );
  }

  const stale = asOfMs !== null && isStale(asOfMs, nowMs);
  const label = tileKind === "in_progress" ? "Now" : "Next";

  return (
    <div
      data-testid="context-tile"
      className={`rounded-lg border p-4 text-sm ${
        stale ? "border-amber-700 text-amber-200" : "border-slate-800 text-slate-200"
      }`}
    >
      <p className="text-xs uppercase tracking-wide text-slate-400">{label}</p>
      {tileEvent.htmlLink ? (
        <a
          href={tileEvent.htmlLink}
          target="_blank"
          rel="noopener noreferrer"
          className="font-medium underline"
        >
          {tileEvent.title}
        </a>
      ) : (
        <p className="font-medium">{tileEvent.title}</p>
      )}
      <p className="text-xs text-slate-400">
        {tileEvent.allDay
          ? "All day"
          : `${formatEventTime(tileEvent.startMs)}–${formatEventTime(tileEvent.endMs)}`}
      </p>
      {asOfMs !== null && (
        <p className={`mt-2 text-xs ${stale ? "text-amber-300" : "text-slate-500"}`}>
          {stale ? "Stale — " : ""}as of {formatAsOf(asOfMs, nowMs)}
        </p>
      )}
    </div>
  );
}
