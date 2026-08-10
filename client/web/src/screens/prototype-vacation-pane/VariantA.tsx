// PROTOTYPE — throwaway. Delete with the rest of this directory (#121).
//
// A — "Countdown tile". One small card in the context panel, in the
// `ContextTile` idiom the calendar already owns: mono kind label, one
// sentence answer, the dates under it, an as-of footer that turns amber
// rather than hiding.
//
// The bet: this is furniture. You do not act on it, you do not need it
// across the room, and it should cost the same screen space as the calendar
// tile sitting next to it. It shows exactly ONE trip — the next one — and
// says nothing about the shape of the year.

import { Badge } from "../../components/core/Badge";
import { Card } from "../../components/core/Card";
import { Icon } from "../../components/core/Icon";
import { countdown, dateRange, dayLabel, emptySentence, sentence } from "./countdown";
import type { VacationAnswer } from "./countdown";

export const NAME = "Countdown tile";

const META: React.CSSProperties = {
  font: "var(--type-meta)",
  letterSpacing: "var(--tracking-meta)",
  textTransform: "uppercase",
};

/** The two things the headline says — the place and the count — set
 * identically, so neither reads as a caption on the other. */
const HEADLINE: React.CSSProperties = {
  font: "var(--weight-bold) 34px/1 var(--font-display)",
  letterSpacing: "var(--tracking-display)",
  color: "var(--text-primary)",
};

/** The words between them, which are grammar rather than answer. */
const JOIN: React.CSSProperties = {
  font: "var(--type-body)",
  color: "var(--text-secondary)",
};

export function VariantA({ answer, nowMs }: { answer: VacationAnswer; nowMs: number }) {
  // An unbound calendar means the human step in the slice was never done.
  // The pane does not exist rather than nagging: nothing is wrong, there is
  // simply no question to answer yet.
  if (answer.state === "unbound") return null;

  const trip = answer.next;
  const underWay = trip !== null && trip.phase !== "upcoming";
  const stale = answer.staleness.stale;

  return (
    <div>
      <span className="hb-meta">next trip</span>
      <Card
        accent={underWay}
        padding="var(--space-5)"
        style={{
          marginTop: "var(--space-4)",
          display: "flex",
          flexDirection: "column",
          gap: "var(--space-3)",
          borderColor: stale ? "var(--status-warn-fg)" : undefined,
        }}
      >
        <span
          style={{
            ...META,
            display: "inline-flex",
            alignItems: "center",
            gap: "var(--space-3)",
            color: underWay ? "var(--text-brand)" : "var(--text-muted)",
          }}
        >
          <Icon name={underWay ? "radio" : "calendar-clock"} size={13} />
          trips
        </span>

        {trip ? (
          <>
            {trip.phase === "upcoming" ? (
              // "Lisbon in 16 days" — the place first, because the place is
              // what the question is actually about; the number is only how
              // far away it is. Both are set at display size and the two
              // joining words are not, so the line reads as two facts with a
              // hinge between them rather than as six words of equal weight.
              <p
                style={{
                  display: "flex",
                  alignItems: "baseline",
                  flexWrap: "wrap",
                  gap: "var(--space-3)",
                }}
              >
                <span style={HEADLINE}>{trip.name}</span>
                <span style={JOIN}>in</span>
                <span style={{ ...HEADLINE, fontVariantNumeric: "tabular-nums" }}>
                  {countdown(trip.daysUntil).value}
                </span>
                <span style={JOIN}>{countdown(trip.daysUntil).unit}</span>
              </p>
            ) : (
              <p
                style={{
                  font: "var(--weight-bold) 20px/1.15 var(--font-display)",
                  letterSpacing: "var(--tracking-heading)",
                  color: "var(--text-primary)",
                }}
              >
                {sentence(trip)}
              </p>
            )}

            <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
              {dateRange(trip)} · {trip.lengthDays} days
            </p>
            {trip.event.location ? (
              <p style={{ font: "var(--type-body-sm)", color: "var(--text-muted)" }}>
                {trip.event.location}
              </p>
            ) : null}

            {/* Under way, the day count that matters is the one running out,
                not the one that ran down. A badge repeating the headline
                would be two of the same fact; a badge counting the days left
                is a second one. */}
            {trip.phase === "under_way" ? (
              <div>
                <Badge tone="brand" icon="calendar" mono>
                  day {trip.lengthDays - trip.daysLeft + 1} of {trip.lengthDays}
                </Badge>
              </div>
            ) : null}

            {/* Every trip after this one, in full. A "+1 more" would be the
                pane withholding something it already has in hand — and the
                rest of the queue is short by nature: these are vacations, not
                a feed. Name left, date right, so the dates form a column the
                eye can run down; no "then" on the first row, because the
                rows are already in order and the word only made row one look
                different from the rest. */}
            {answer.later.length > 0 ? (
              <div
                style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}
              >
                {answer.later.map((later) => (
                  <p
                    key={later.event.providerEventId}
                    style={{
                      ...META,
                      display: "flex",
                      justifyContent: "space-between",
                      gap: "var(--space-4)",
                      color: "var(--text-muted)",
                    }}
                  >
                    <span>{later.name}</span>
                    <span style={{ whiteSpace: "nowrap" }}>
                      {dayLabel(later.event.startMs, nowMs)}
                    </span>
                  </p>
                ))}
              </div>
            ) : null}
          </>
        ) : (
          <p style={{ font: "var(--type-body)", color: "var(--text-muted)" }}>
            {emptySentence(answer)}
          </p>
        )}

        {answer.staleness.label ? (
          <p
            style={{
              ...META,
              marginTop: "var(--space-2)",
              color: stale ? "var(--status-warn-fg)" : "var(--text-muted)",
            }}
          >
            {stale ? "Stale — " : ""}as of {answer.staleness.label}
          </p>
        ) : null}
      </Card>
    </div>
  );
}
