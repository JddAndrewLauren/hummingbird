// PROTOTYPE — throwaway. Delete with the rest of this directory (#122).
//
// C — Three ledgers. It refuses the merge. Booked / Owed / Chosen are three
// separate lists, each chronological across the whole window with the day
// named on the row. The bet: the domain's own distinction — a commitment
// someone else holds you to, a deadline with consequences, a preference you
// chose — is the answer, and interleaving them into one day list hides the
// only thing that tells a reader which rows they can move.
//
// It also makes the asymmetry visible: two of the three lists are facts you
// cannot edit here, and exactly one is yours. So the `scheduled_date`
// affordance is a labelled FIELD on every editable row — a day select with
// "Not planned" as a first-class option, not a chip you toggle — and it
// appears on Owed rows too, because choosing a day for something you owe is
// the whole move this pane is trying to make easy.

import type { ReactNode } from "react";
import { Badge } from "../../components/core/Badge";
import { Card } from "../../components/core/Card";
import { Icon } from "../../components/core/Icon";
import { Select } from "../../components/forms/Select";
import { EmptyState } from "../../components/feedback/EmptyState";
import { entryUrgency, timeLabel, type WindowEntry } from "./weekend";
import type { VariantProps } from "./variant";

export const NAME = "Three ledgers";
export const SLOT = "aside";

const URGENCY_COLOR = {
  calm: "var(--urgency-calm)",
  soon: "var(--urgency-soon)",
  now: "var(--urgency-now)",
  overdue: "var(--urgency-overdue)",
} as const;

const NOT_PLANNED = "";

function Ledger({
  title,
  icon,
  count,
  children,
}: {
  title: string;
  icon: "calendar-clock" | "flag" | "calendar";
  count: number;
  children: ReactNode;
}) {
  return (
    <Card padding="var(--space-5)" style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
      <div style={{ display: "flex", alignItems: "center", gap: "var(--space-3)" }}>
        <Icon name={icon} size={16} color="var(--text-secondary)" />
        <span style={{ flex: 1, font: "var(--type-body-strong)" }}>{title}</span>
        <Badge mono>{count}</Badge>
      </div>
      {children}
    </Card>
  );
}

export function VariantC({ window: weekend, nowMs, calendar, onPlan }: VariantProps) {
  const all = weekend.days.flatMap((day) =>
    day.entries.map((entry) => ({ entry, dayLabel: day.label, dayKey: day.key })),
  );
  const booked = all.filter((row) => row.entry.kind === "event");
  const owed = all.filter((row) => row.entry.kind === "due");
  const chosen = all.filter((row) => row.entry.kind === "scheduled");

  const dayOptions = [
    { value: NOT_PLANNED, label: "Not planned" },
    ...weekend.days.map((day) => ({ value: day.key, label: day.label })),
  ];

  function planField(entry: WindowEntry, current: string) {
    if (!entry.item) return null;
    const itemId = entry.item.id;
    return (
      <Select
        size="sm"
        options={dayOptions}
        value={current}
        aria-label={`Do-date for ${entry.title}`}
        onChange={(changeEvent) =>
          onPlan(itemId, changeEvent.target.value === NOT_PLANNED ? null : changeEvent.target.value)
        }
        style={{ width: 132, flex: "0 0 auto" }}
      />
    );
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
      <span className="hb-meta">
        weekend · {weekend.days.map((day) => day.dateLabel).join(" – ")}
      </span>

      <Ledger title="On the calendar" icon="calendar-clock" count={booked.length}>
        {calendar === null ? (
          <span style={{ font: "var(--type-body-sm)", color: "var(--status-warn-fg)" }}>
            No calendar connected. This list is empty because nothing is being polled, not because
            the weekend is free.
          </span>
        ) : booked.length === 0 ? (
          <span style={{ font: "var(--type-body-sm)", color: "var(--text-muted)" }}>
            Nothing booked.
          </span>
        ) : (
          booked.map((row) => (
            <div key={row.entry.id} style={{ display: "flex", alignItems: "baseline", gap: "var(--space-4)" }}>
              <span className="hb-meta" style={{ width: 68, flex: "0 0 auto", color: "var(--text-muted)" }}>
                {row.dayLabel.slice(0, 3)} {timeLabel(row.entry).split(" – ")[0]}
              </span>
              <span style={{ flex: 1, minWidth: 0, font: "var(--type-body-sm)" }}>{row.entry.title}</span>
            </div>
          ))
        )}
        {calendar ? (
          <span className="hb-meta" style={{ color: calendar.stale ? "var(--status-warn-fg)" : "var(--text-muted)" }}>
            {calendar.stale ? `stale — as of ${calendar.asOf}` : `as of ${calendar.asOf}`}
          </span>
        ) : null}
      </Ledger>

      <Ledger title="Due this weekend" icon="flag" count={owed.length}>
        {owed.length === 0 ? (
          <span style={{ font: "var(--type-body-sm)", color: "var(--text-muted)" }}>
            Nothing falls due.
          </span>
        ) : (
          owed.map((row) => (
            <div key={row.entry.id} style={{ display: "flex", alignItems: "center", gap: "var(--space-4)" }}>
              <span
                title={`Urgency: ${entryUrgency(row.entry, nowMs)}`}
                style={{
                  width: 6,
                  height: 6,
                  flex: "0 0 auto",
                  borderRadius: "50%",
                  background: URGENCY_COLOR[entryUrgency(row.entry, nowMs)],
                }}
              />
              <span style={{ flex: 1, minWidth: 0 }}>
                <span style={{ display: "block", font: "var(--type-body-sm)" }}>{row.entry.title}</span>
                <span className="hb-meta" style={{ color: "var(--text-muted)" }}>
                  {row.dayLabel.slice(0, 3)} · {timeLabel(row.entry)}
                </span>
              </span>
              {planField(row.entry, row.entry.alsoScheduledOn ?? NOT_PLANNED)}
            </div>
          ))
        )}
      </Ledger>

      <Ledger title="Planned to do" icon="calendar" count={chosen.length}>
        {chosen.length === 0 ? (
          <EmptyState
            compact
            icon="feather"
            headingLevel={2}
            title="No do-dates"
            body="Nothing has been given a day yet. Set one from the list above."
          />
        ) : (
          chosen.map((row) => (
            <div key={row.entry.id} style={{ display: "flex", alignItems: "center", gap: "var(--space-4)" }}>
              <span style={{ flex: 1, minWidth: 0 }}>
                <span style={{ display: "block", font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
                  {row.entry.title}
                </span>
                <span className="hb-meta" style={{ color: "var(--text-muted)" }}>
                  {row.entry.deadlineOutsideWindow
                    ? `due ${row.entry.deadlineOutsideWindow}`
                    : "no deadline"}
                </span>
              </span>
              {planField(row.entry, row.dayKey)}
            </div>
          ))
        )}
      </Ledger>
    </div>
  );
}
