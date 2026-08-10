// PROTOTYPE — throwaway. Delete with the rest of this directory (#122).
//
// A — The weekend card. ONE card for the whole window, with the days as
// sections inside it, everything interleaved chronologically within a day.
// The bet: the weekend is one thing you are asking about, so it is one card
// in the context panel; the day is a divider inside the answer, not a
// separate answer. Events / deadlines / do-dates stay mixed, distinguished
// by glyph and colour, because that is how a day is actually lived.
//
// (It rendered one card per day until the operator asked for a single
// weekend card. What that change bought and cost is finding 10 in NOTES.md.)
//
// The `scheduled_date` affordance is INLINE on the row: a due row with no
// do-date offers the remaining days as chips; a row that has one shows it as
// a filled chip you click again to clear. Bet: planning happens while
// reading the weekend, not in a separate mode.

import { Badge } from "../../components/core/Badge";
import { Card } from "../../components/core/Card";
import { Icon } from "../../components/core/Icon";
import { EmptyState } from "../../components/feedback/EmptyState";
import { entryUrgency, shortDayLabel, timeLabel, type WindowEntry } from "./weekend";
import type { VariantProps } from "./variant";

export const NAME = "Weekend card";
export const SLOT = "aside";

const URGENCY_COLOR = {
  calm: "var(--urgency-calm)",
  soon: "var(--urgency-soon)",
  now: "var(--urgency-now)",
  overdue: "var(--urgency-overdue)",
} as const;

function KindGlyph({ entry, nowMs }: { entry: WindowEntry; nowMs: number }) {
  if (entry.kind === "event") {
    return <Icon name="calendar-clock" size={16} color="var(--text-secondary)" />;
  }
  if (entry.kind === "due") {
    // The flag is the due-date glyph (design README's icon vocabulary), and
    // it is the ONE thing on this row that carries urgency colour — a
    // do-date never does, because a do-date has no consequence.
    return <Icon name="flag" size={16} color={URGENCY_COLOR[entryUrgency(entry, nowMs)]} />;
  }
  return <Icon name="calendar" size={16} color="var(--text-muted)" />;
}

function PlanChips({
  entry,
  dayKeys,
  onPlan,
}: {
  entry: WindowEntry;
  dayKeys: string[];
  onPlan: VariantProps["onPlan"];
}) {
  const item = entry.item;
  if (!item) return null;
  const planned = entry.kind === "scheduled" ? entry.dayKey : (entry.alsoScheduledOn ?? null);

  return (
    <span style={{ display: "inline-flex", gap: "var(--space-2)", flex: "0 0 auto" }}>
      {dayKeys.map((key) => {
        const on = planned === key;
        return (
          <button
            key={key}
            type="button"
            title={on ? "Clear this do-date" : `Plan it for ${shortDayLabel(key)}`}
            onClick={(clickEvent) => {
              clickEvent.stopPropagation();
              onPlan(item.id, on ? null : key);
            }}
            style={{
              height: 20,
              padding: "0 var(--space-3)",
              borderRadius: "var(--radius-pill)",
              cursor: "pointer",
              font: "var(--type-meta)",
              letterSpacing: "var(--tracking-meta)",
              textTransform: "uppercase",
              color: on ? "var(--text-brand)" : "var(--text-muted)",
              background: on ? "var(--accent-quiet)" : "transparent",
              border: `1px solid ${on ? "var(--accent-quiet-border)" : "var(--border-subtle)"}`,
              transition: "background var(--dur-fast) var(--ease-flit)",
            }}
          >
            {shortDayLabel(key)}
          </button>
        );
      })}
    </span>
  );
}

function EntryRow({
  entry,
  dayKeys,
  nowMs,
  onPlan,
}: {
  entry: WindowEntry;
  dayKeys: string[];
  nowMs: number;
  onPlan: VariantProps["onPlan"];
}) {
  const meta = [
    timeLabel(entry),
    entry.event?.calendarName,
    entry.alsoScheduledOn ? `planned ${shortDayLabel(entry.alsoScheduledOn)}` : null,
    entry.deadlineOutsideWindow ? `due ${entry.deadlineOutsideWindow}` : null,
  ].filter(Boolean);

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: "var(--space-4)",
        minHeight: "var(--row-height)",
        padding: "var(--space-3) 0",
      }}
    >
      <KindGlyph entry={entry} nowMs={nowMs} />
      <span style={{ flex: 1, minWidth: 0 }}>
        <span
          style={{
            display: "block",
            font: "var(--type-body-sm)",
            color: entry.kind === "scheduled" ? "var(--text-secondary)" : "var(--text-primary)",
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {entry.title}
        </span>
        <span className="hb-meta" style={{ color: "var(--text-muted)" }}>
          {meta.join(" · ")}
        </span>
      </span>
      {entry.item ? <PlanChips entry={entry} dayKeys={dayKeys} onPlan={onPlan} /> : null}
    </div>
  );
}

export function VariantA({ window: weekend, nowMs, calendar, onPlan }: VariantProps) {
  const dayKeys = weekend.days.map((day) => day.key);
  const empty = weekend.days.every((day) => day.entries.length === 0);
  // With one day left there is nothing for a day heading to separate, so the
  // remaining day is named in the card header instead and the sections
  // collapse away entirely — a heading over the only section is furniture.
  const oneDay = weekend.days.length === 1;
  const soleDay = oneDay ? weekend.days[0] : null;

  // The card names itself, so the mono kicker that used to sit above it is
  // gone — it said "this weekend" directly above a card headed "Sat – Sun",
  // which was the same fact twice. Everything the kicker carried (the dates,
  // and whether the weekend is already under way) is in the meta line.
  const meta = [
    soleDay?.label,
    soleDay ? soleDay.dateLabel : weekend.days.map((day) => day.dateLabel).join(" – "),
    // "under way" only where it adds something: naming a single remaining
    // day under a card headed "This Weekend" already says the weekend
    // started, and the third clause wrapped the meta onto three lines in a
    // 320px panel.
    weekend.inProgress && !soleDay ? "under way" : null,
  ].filter(Boolean);

  return (
    <div>
      <Card
        padding="var(--space-5)"
        style={{
          display: "flex",
          flexDirection: "column",
          gap: "var(--space-4)",
        }}
      >
        <div style={{ display: "flex", alignItems: "baseline", justifyContent: "space-between", gap: "var(--space-3)" }}>
          <span
            style={{
              font: "var(--type-h3)",
              color: "var(--text-primary)",
              whiteSpace: "nowrap",
            }}
          >
            This Weekend
          </span>
          <span className="hb-meta" style={{ color: "var(--text-muted)", textAlign: "right" }}>
            {meta.join(" · ")}
          </span>
        </div>

        {calendar === null ? (
          <span className="hb-meta" style={{ color: "var(--status-warn-fg)" }}>
            no calendar — deadlines and do-dates only
          </span>
        ) : null}

        {empty ? (
          <EmptyState
            compact
            icon="feather"
            headingLevel={2}
            title="A clear weekend"
            body="Nothing on the calendar, nothing due, nothing planned."
          />
        ) : (
          weekend.days.map((day, index) => (
            <div
              key={day.key}
              style={{
                display: "flex",
                flexDirection: "column",
                // The one divider in the card: days are sections, not cards,
                // so they are separated by a rule rather than by elevation
                // (never more than two elevations in a region).
                borderTop: index === 0 ? "none" : "1px solid var(--border-subtle)",
                paddingTop: index === 0 ? 0 : "var(--space-4)",
              }}
            >
              {oneDay ? null : (
                <div style={{ display: "flex", alignItems: "baseline", justifyContent: "space-between" }}>
                  <span style={{ font: "var(--type-body-strong)", color: "var(--text-primary)" }}>
                    {day.label}
                  </span>
                  <span className="hb-meta" style={{ color: "var(--text-muted)" }}>
                    {day.entries.length === 0 ? "clear" : `${day.entries.length} things`}
                  </span>
                </div>
              )}

              {day.entries.length === 0 ? (
                <span
                  style={{
                    font: "var(--type-body-sm)",
                    color: "var(--text-muted)",
                    padding: "var(--space-3) 0",
                  }}
                >
                  Nothing booked, nothing due.
                </span>
              ) : (
                day.entries.map((entry) => (
                  <EntryRow
                    key={entry.id}
                    entry={entry}
                    dayKeys={dayKeys}
                    nowMs={nowMs}
                    onPlan={onPlan}
                  />
                ))
              )}
            </div>
          ))
        )}

        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: "var(--space-3)",
            flexWrap: "wrap",
            paddingTop: "var(--space-4)",
            borderTop: "1px solid var(--border-subtle)",
          }}
        >
          <Badge icon="calendar-clock">on the calendar</Badge>
          <Badge icon="flag" tone="warn">
            due
          </Badge>
          <Badge icon="calendar">planned</Badge>
          {calendar ? (
            <span
              className="hb-meta"
              style={{
                marginLeft: "auto",
                color: calendar.stale ? "var(--status-warn-fg)" : "var(--text-muted)",
              }}
            >
              {calendar.stale ? `stale — as of ${calendar.asOf}` : `as of ${calendar.asOf}`}
            </span>
          ) : null}
        </div>
      </Card>
    </div>
  );
}
