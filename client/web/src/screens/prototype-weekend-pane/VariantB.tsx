// PROTOTYPE — throwaway. Delete with the rest of this directory (#122).
//
// B — The ribbon. A full-width band above the frontier: one time-proportional
// column per weekend day, booked time drawn as blocks against a 7am–11pm
// axis, so the answer a reader takes away is *how much room is left*, not a
// list of names. The bet: "what are my plans this weekend" is usually asked
// in order to find out whether something else fits.
//
// Consequently deadlines and do-dates are NOT in the columns — they are not
// blocks of time, and drawing them as if they were would lie about the room.
// A day-anchored deadline sits in the day's footer ("owed by end of day");
// do-dates sit in a tray above the axis ("planned, no time").
//
// The `scheduled_date` affordance is a dedicated strip at the bottom: every
// deadline in the window with no day chosen, each offering the days. Bet:
// planning is its own act, and the pane's one call to action is "you owe
// three things this weekend and have decided when for none of them".

import { Card } from "../../components/core/Card";
import { Icon } from "../../components/core/Icon";
import { entryUrgency, shortDayLabel, unplanned, type WeekendDay, type WindowEntry } from "./weekend";
import type { VariantProps } from "./variant";

export const NAME = "The ribbon";
export const SLOT = "banner";

const AXIS_START_HOUR = 7;
const AXIS_END_HOUR = 23;
const AXIS_HEIGHT = 240;

const URGENCY_COLOR = {
  calm: "var(--urgency-calm)",
  soon: "var(--urgency-soon)",
  now: "var(--urgency-now)",
  overdue: "var(--urgency-overdue)",
} as const;

function hourOf(ms: number, dayStartMs: number): number {
  return (ms - dayStartMs) / 3_600_000;
}

function offsetFor(hour: number): number {
  const clamped = Math.min(Math.max(hour, AXIS_START_HOUR), AXIS_END_HOUR);
  return ((clamped - AXIS_START_HOUR) / (AXIS_END_HOUR - AXIS_START_HOUR)) * AXIS_HEIGHT;
}

/** The one computed sentence this variant exists to say. Busy intervals are
 * merged first, so two overlapping bookings do not read as two gaps. */
function roomLabel(day: WeekendDay): string {
  const busy = day.entries
    .filter((entry) => entry.kind === "event" && entry.event && !entry.event.allDay)
    .map((entry) => [hourOf(entry.event!.startMs, day.startMs), hourOf(entry.event!.endMs, day.startMs)] as const)
    .map(([from, to]) => [Math.max(from, AXIS_START_HOUR), Math.min(to, AXIS_END_HOUR)] as const)
    .filter(([from, to]) => to > from)
    .sort((a, b) => a[0] - b[0]);

  const merged: Array<[number, number]> = [];
  for (const [from, to] of busy) {
    const last = merged[merged.length - 1];
    if (last && from <= last[1]) last[1] = Math.max(last[1], to);
    else merged.push([from, to]);
  }

  let bestFrom = AXIS_START_HOUR;
  let best = 0;
  let cursor = AXIS_START_HOUR;
  for (const [from, to] of merged) {
    if (from - cursor > best) {
      best = from - cursor;
      bestFrom = cursor;
    }
    cursor = Math.max(cursor, to);
  }
  if (AXIS_END_HOUR - cursor > best) {
    best = AXIS_END_HOUR - cursor;
    bestFrom = cursor;
  }

  if (merged.length === 0) return "nothing booked";
  const hour = Math.floor(bestFrom);
  const suffix = hour >= 12 ? "pm" : "am";
  const shown = hour % 12 === 0 ? 12 : hour % 12;
  return `${Math.round(best)}h clear from ${shown}${suffix}`;
}

function EventBlock({ entry, dayStartMs }: { entry: WindowEntry; dayStartMs: number }) {
  const event = entry.event;
  if (!event) return null;
  const top = offsetFor(hourOf(event.startMs, dayStartMs));
  const bottom = offsetFor(hourOf(event.endMs, dayStartMs));
  return (
    <div
      title={`${event.title} · ${event.calendarName}`}
      style={{
        position: "absolute",
        left: 0,
        right: 0,
        top,
        height: Math.max(bottom - top, 14),
        padding: "2px var(--space-3)",
        background: "var(--surface-quiet)",
        borderLeft: "2px solid var(--text-secondary)",
        borderRadius: "var(--radius-sm)",
        overflow: "hidden",
        font: "var(--type-body-sm)",
        color: "var(--text-primary)",
        whiteSpace: "nowrap",
        textOverflow: "ellipsis",
      }}
    >
      {event.title}
    </div>
  );
}

function DueMark({ entry, dayStartMs, nowMs }: { entry: WindowEntry; dayStartMs: number; nowMs: number }) {
  const top = offsetFor(hourOf(entry.atMs, dayStartMs));
  const color = URGENCY_COLOR[entryUrgency(entry, nowMs)];
  return (
    <div
      title={entry.title}
      style={{
        position: "absolute",
        left: 0,
        right: 0,
        top,
        display: "flex",
        alignItems: "center",
        gap: "var(--space-2)",
        borderTop: `1px dashed ${color}`,
        pointerEvents: "none",
      }}
    >
      <Icon name="flag" size={13} color={color} />
      <span className="hb-meta" style={{ color }}>
        {entry.title}
      </span>
    </div>
  );
}

export function VariantB({ window: weekend, nowMs, calendar, onPlan }: VariantProps) {
  const dayKeys = weekend.days.map((day) => day.key);
  const toPlan = unplanned(weekend);

  return (
    <Card
      elevation={2}
      padding="var(--space-6)"
      style={{ display: "flex", flexDirection: "column", gap: "var(--space-5)" }}
    >
      <div style={{ display: "flex", alignItems: "baseline", justifyContent: "space-between" }}>
        <h2 style={{ font: "var(--type-h2)", letterSpacing: "var(--tracking-heading)", margin: 0 }}>
          {weekend.inProgress ? "The rest of this weekend" : "This coming weekend"}
        </h2>
        <span
          className="hb-meta"
          style={{ color: calendar?.stale ? "var(--status-warn-fg)" : "var(--text-muted)" }}
        >
          {calendar === null
            ? "no calendar — deadlines only"
            : calendar.stale
              ? `stale — as of ${calendar.asOf}`
              : `as of ${calendar.asOf}`}
        </span>
      </div>

      <div style={{ display: "flex", gap: "var(--space-6)" }}>
        {weekend.days.map((day) => {
          const timed = day.entries.filter((entry) => entry.kind === "event" && entry.event && !entry.event.allDay);
          const spans = day.entries.filter((entry) => entry.kind === "event" && entry.event?.allDay);
          const planned = day.entries.filter((entry) => entry.kind === "scheduled");
          const dueTimed = day.entries.filter((entry) => entry.kind === "due" && entry.anchor === "time");
          const dueEndOfDay = day.entries.filter((entry) => entry.kind === "due" && entry.anchor === "day");

          return (
            <div key={day.key} style={{ flex: 1, minWidth: 0 }}>
              <div style={{ display: "flex", alignItems: "baseline", gap: "var(--space-3)" }}>
                <span style={{ font: "var(--type-h3)" }}>{day.label}</span>
                <span className="hb-meta" style={{ color: "var(--text-muted)" }}>
                  {roomLabel(day)}
                </span>
              </div>

              {spans.map((entry) => (
                <div
                  key={entry.id}
                  style={{
                    marginTop: "var(--space-3)",
                    padding: "2px var(--space-3)",
                    borderRadius: "var(--radius-sm)",
                    background: "var(--accent-quiet)",
                    border: "1px solid var(--accent-quiet-border)",
                    font: "var(--type-body-sm)",
                    color: "var(--text-brand)",
                  }}
                >
                  {entry.title} — all day
                </div>
              ))}

              {planned.length > 0 ? (
                <div style={{ marginTop: "var(--space-3)", display: "flex", gap: "var(--space-2)", flexWrap: "wrap" }}>
                  {planned.map((entry) => (
                    <button
                      key={entry.id}
                      type="button"
                      title="Clear this do-date"
                      onClick={() => entry.item && onPlan(entry.item.id, null)}
                      style={{
                        display: "inline-flex",
                        alignItems: "center",
                        gap: "var(--space-2)",
                        height: 22,
                        padding: "0 var(--space-3)",
                        borderRadius: "var(--radius-pill)",
                        border: "1px dashed var(--border-default)",
                        background: "transparent",
                        color: "var(--text-secondary)",
                        font: "var(--type-body-sm)",
                        cursor: "pointer",
                      }}
                    >
                      <Icon name="calendar" size={13} />
                      {entry.title}
                      <Icon name="x" size={12} />
                    </button>
                  ))}
                </div>
              ) : null}

              <div
                style={{
                  position: "relative",
                  height: AXIS_HEIGHT,
                  marginTop: "var(--space-4)",
                  borderTop: "1px solid var(--border-subtle)",
                  borderBottom: "1px solid var(--border-subtle)",
                  background:
                    "repeating-linear-gradient(to bottom, transparent 0, transparent 29px, var(--border-subtle) 29px, var(--border-subtle) 30px)",
                }}
              >
                {timed.map((entry) => (
                  <EventBlock key={entry.id} entry={entry} dayStartMs={day.startMs} />
                ))}
                {dueTimed.map((entry) => (
                  <DueMark key={entry.id} entry={entry} dayStartMs={day.startMs} nowMs={nowMs} />
                ))}
                {timed.length === 0 && dueTimed.length === 0 ? (
                  <span
                    className="hb-meta"
                    style={{
                      position: "absolute",
                      inset: 0,
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "center",
                      color: "var(--text-muted)",
                    }}
                  >
                    open
                  </span>
                ) : null}
              </div>

              {dueEndOfDay.length > 0 ? (
                <div style={{ marginTop: "var(--space-3)", display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
                  {dueEndOfDay.map((entry) => (
                    <span
                      key={entry.id}
                      style={{
                        display: "inline-flex",
                        alignItems: "center",
                        gap: "var(--space-2)",
                        font: "var(--type-body-sm)",
                        color: "var(--text-primary)",
                      }}
                    >
                      <Icon name="flag" size={13} color={URGENCY_COLOR[entryUrgency(entry, nowMs)]} />
                      {entry.title}
                      <span className="hb-meta" style={{ color: "var(--text-muted)" }}>
                        by end of day
                      </span>
                    </span>
                  ))}
                </div>
              ) : null}
            </div>
          );
        })}
      </div>

      {toPlan.length > 0 ? (
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            gap: "var(--space-3)",
            paddingTop: "var(--space-4)",
            borderTop: "1px solid var(--border-subtle)",
          }}
        >
          <span className="hb-meta">
            {toPlan.length} due this weekend · no day chosen
          </span>
          {toPlan.map((item) => (
            <div key={item.id} style={{ display: "flex", alignItems: "center", gap: "var(--space-4)" }}>
              <span style={{ flex: 1, minWidth: 0, font: "var(--type-body-sm)" }}>{item.title}</span>
              {dayKeys.map((key) => (
                <button
                  key={key}
                  type="button"
                  onClick={() => onPlan(item.id, key)}
                  style={{
                    height: 22,
                    padding: "0 var(--space-4)",
                    borderRadius: "var(--radius-pill)",
                    border: "1px solid var(--border-default)",
                    background: "transparent",
                    color: "var(--text-secondary)",
                    font: "var(--type-body-sm)",
                    cursor: "pointer",
                  }}
                >
                  Plan {shortDayLabel(key)}
                </button>
              ))}
            </div>
          ))}
        </div>
      ) : null}
    </Card>
  );
}
