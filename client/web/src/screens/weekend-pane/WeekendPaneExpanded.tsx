import { Badge } from "../../components/core/Badge";
import { Button } from "../../components/core/Button";
import { Card } from "../../components/core/Card";
import { Icon } from "../../components/core/Icon";
import { EmptyState } from "../../components/feedback/EmptyState";
import type { QuestionInputs } from "../questions/contract";
import {
  entryUrgency,
  shortDayLabel,
  timeLabel,
  weekendCalendarRead,
  weekendView,
  type WeekendWindow,
  type WindowEntry,
} from "./weekend";

// The weekend-plans pane's own expanded rendering — the *only* part of this
// question the shell does not draw. Carried over from
// `screens/prototype-weekend-pane/`'s Variant A, which won on 2026-08-10:
// one card for the whole window, the days as sections inside it, events /
// deadlines / do-dates interleaved chronologically within a day and
// distinguished by glyph rather than by a second card each.
//
// The `scheduled_date` affordance is inline on each row: a due row with no
// do-date offers the remaining days as chips; a row that already has one
// shows it filled and clears it on a second click. Planning happens while
// reading the weekend, never in a separate mode.

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
  onSetScheduledDate,
}: {
  entry: WindowEntry;
  dayKeys: string[];
  onSetScheduledDate: (itemId: string, date: string | null) => void;
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
              onSetScheduledDate(item.id, on ? null : key);
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
  onSetScheduledDate,
}: {
  entry: WindowEntry;
  dayKeys: string[];
  nowMs: number;
  onSetScheduledDate?: (itemId: string, date: string | null) => void;
}) {
  const meta = [
    timeLabel(entry),
    entry.event?.calendarId,
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
      {entry.item && onSetScheduledDate ? (
        <PlanChips entry={entry} dayKeys={dayKeys} onSetScheduledDate={onSetScheduledDate} />
      ) : null}
    </div>
  );
}

function WeekendCard({
  window: weekend,
  nowMs,
  stale,
  onSetScheduledDate,
}: {
  window: WeekendWindow;
  nowMs: number;
  stale: boolean;
  onSetScheduledDate?: (itemId: string, date: string | null) => void;
}) {
  const dayKeys = weekend.days.map((day) => day.key);
  const empty = weekend.days.every((day) => day.entries.length === 0);

  const meta = [
    weekend.days.map((day) => day.dateLabel).join(" – "),
    weekend.underWay ? "under way" : null,
  ].filter(Boolean);

  return (
    <Card
      accent
      padding="var(--space-5)"
      style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "baseline",
          justifyContent: "space-between",
          gap: "var(--space-3)",
        }}
      >
        <span style={{ font: "var(--type-h3)", color: "var(--text-primary)", whiteSpace: "nowrap" }}>
          This Weekend
        </span>
        <span className="hb-meta" style={{ color: "var(--text-muted)", textAlign: "right" }}>
          {meta.join(" · ")}
        </span>
      </div>

      {stale ? (
        <span className="hb-meta" style={{ color: "var(--status-warn-fg)" }}>
          calendar hasn't synced in a while — showing the last answer
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
              borderTop: index === 0 ? "none" : "1px solid var(--border-subtle)",
              paddingTop: index === 0 ? 0 : "var(--space-4)",
            }}
          >
            <div style={{ display: "flex", alignItems: "baseline", justifyContent: "space-between" }}>
              <span style={{ font: "var(--type-body-strong)", color: "var(--text-primary)" }}>
                {day.label}
              </span>
              <span className="hb-meta" style={{ color: "var(--text-muted)" }}>
                {day.entries.length === 0 ? "clear" : `${day.entries.length} things`}
              </span>
            </div>

            {day.entries.length === 0 ? (
              <span style={{ font: "var(--type-body-sm)", color: "var(--text-muted)", padding: "var(--space-3) 0" }}>
                Nothing booked, nothing due.
              </span>
            ) : (
              day.entries.map((entry) => (
                <EntryRow
                  key={entry.id}
                  entry={entry}
                  dayKeys={dayKeys}
                  nowMs={nowMs}
                  onSetScheduledDate={onSetScheduledDate}
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
      </div>
    </Card>
  );
}

export function WeekendPaneExpanded({
  inputs,
  onSetupNavigate,
  onSetScheduledDate,
}: {
  subjectKey: string;
  inputs: QuestionInputs;
  onSetupNavigate?: () => void;
  onSetScheduledDate?: (itemId: string, date: string | null) => void;
}) {
  // #122 review fix: `!calendarConnected` is the ONLY path to the setup
  // prompt. A missing calendar-arm entry or a connected `"not_read"` read
  // (never polled yet, offline, `needsReconnect`) are both "the table
  // hasn't answered yet", not "never set up" — telling an already-connected
  // reader to go set the pane up is a wrong answer, not a slow one
  // (`waste.ts`'s own "unread" reasoning).
  if (!inputs.calendarConnected) {
    return (
      <Card padding="var(--space-3)">
        <EmptyState
          compact
          icon="help-circle"
          headingLevel={3}
          title="What are my plans this weekend?"
          body="Connect a calendar to see events alongside what's due and planned."
          action={
            onSetupNavigate ? (
              <Button variant="secondary" iconLeft="settings" onClick={onSetupNavigate}>
                Open Settings
              </Button>
            ) : undefined
          }
        />
      </Card>
    );
  }

  const read = weekendCalendarRead(inputs);

  if (read === undefined || read.state === "not_read") {
    return (
      <Card padding="var(--space-3)">
        <EmptyState
          compact
          icon="cloud-fog"
          headingLevel={3}
          title="Checking your calendar"
          body="Waiting on this device's calendar sync."
        />
      </Card>
    );
  }

  const view = weekendView(inputs);
  if (view === null) {
    // Unreachable given `read.state === "read"` above (`weekendView` only
    // returns `null` for `undefined`/`not_read`), kept as a typed fallback
    // rather than a non-null assertion — visibly broken, never a crash.
    return (
      <Card padding="var(--space-3)">
        <EmptyState compact icon="cloud-fog" headingLevel={3} title="No answer yet" body="" />
      </Card>
    );
  }

  const stale = read.freshness.kind === "unknown";

  return (
    <WeekendCard
      window={view}
      nowMs={inputs.nowMs}
      stale={stale}
      onSetScheduledDate={onSetScheduledDate}
    />
  );
}
