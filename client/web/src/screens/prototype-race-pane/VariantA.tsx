// PROTOTYPE — throwaway. Delete with the rest of this directory (#119).
//
// A — "Series tile". One card per followed series, stacked in the context
// panel, in the ContextTile idiom the calendar already owns: a mono kind
// label, one sentence answer, the session line under it, an as-of footer
// that turns amber rather than hiding. The pane is furniture; the alert lane
// is what interrupts.

import { Badge } from "../../components/core/Badge";
import { Card } from "../../components/core/Card";
import { Icon } from "../../components/core/Icon";
import { EmptyState } from "../../components/feedback/EmptyState";
import { clock, dayLabel, sentence } from "./countdown";
import type { SeriesAnswer } from "./countdown";

export const NAME = "Series tile";
export const SLOT = "aside";

function Tile({ answer, nowMs }: { answer: SeriesAnswer; nowMs: number }) {
  if (!answer.row) {
    return (
      <Card padding="var(--space-5)">
        <span className="hb-meta">{answer.seriesLabel}</span>
        <p
          style={{
            marginTop: "var(--space-3)",
            font: "var(--type-body-sm)",
            color: "var(--text-muted)",
          }}
        >
          Never polled. Nothing to answer with yet.
        </p>
      </Card>
    );
  }

  const next = answer.next;
  const stale = answer.staleness.stale;
  const live = answer.alerts.length > 0;

  return (
    <Card
      accent={live}
      padding="var(--space-5)"
      style={{
        display: "flex",
        flexDirection: "column",
        gap: "var(--space-3)",
        borderColor: stale ? "var(--status-warn-fg)" : undefined,
      }}
    >
      <span
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: "var(--space-3)",
          font: "var(--type-meta)",
          letterSpacing: "var(--tracking-meta)",
          textTransform: "uppercase",
          color: next?.underWay ? "var(--text-brand)" : "var(--text-muted)",
        }}
      >
        <Icon name={next?.underWay ? "radio" : "calendar-clock"} size={13} />
        {answer.seriesLabel}
      </span>

      {next ? (
        <>
          <p
            style={{
              font: "var(--weight-bold) 20px/1.15 var(--font-display)",
              letterSpacing: "var(--tracking-heading)",
              color: "var(--text-primary)",
            }}
          >
            {sentence(answer)}
          </p>
          {/* The headline counts to race day; this line carries the other
              fact, whichever one that is. On a quiet week that is the session
              that actually happens first (Friday practice). While a session
              is running the headline has already said so, so this line goes
              back to answering "and when is the race". */}
          <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
            {next.underWay && answer.race
              ? `${answer.race.session.label} · ${dayLabel(answer.race.session.startsAtMs, nowMs)} ${clock(answer.race.session.startsAtMs)}`
              : `${next.session.label} · ${dayLabel(next.session.startsAtMs, nowMs)} ${clock(next.session.startsAtMs)}`}
          </p>
          <p style={{ font: "var(--type-body-sm)", color: "var(--text-muted)" }}>{next.locality}</p>
          {live ? (
            <div style={{ marginTop: "var(--space-2)" }}>
              <Badge tone="danger" icon="siren" mono>
                starting soon
              </Badge>
            </div>
          ) : null}
        </>
      ) : (
        <p style={{ font: "var(--type-body)", color: "var(--text-muted)" }}>
          No scheduled sessions.
        </p>
      )}

      {answer.staleness.label ? (
        <p
          style={{
            marginTop: "var(--space-2)",
            font: "var(--type-meta)",
            letterSpacing: "var(--tracking-meta)",
            textTransform: "uppercase",
            color: stale ? "var(--status-warn-fg)" : "var(--text-muted)",
          }}
        >
          {stale ? "Stale — " : ""}as of {answer.staleness.label}
        </p>
      ) : null}
    </Card>
  );
}

export function VariantA({ answers, nowMs }: { answers: SeriesAnswer[]; nowMs: number }) {
  if (answers.length === 0) {
    return null;
  }
  return (
    <div>
      <span className="hb-meta">next race</span>
      <div
        style={{
          marginTop: "var(--space-4)",
          display: "flex",
          flexDirection: "column",
          gap: "var(--space-4)",
        }}
      >
        {answers.every((answer) => !answer.row) ? (
          <Card padding="var(--space-3)">
            <EmptyState
              compact
              icon="calendar-clock"
              headingLevel={2}
              title="No schedule yet"
              body="The race cron has not polled a followed series."
            />
          </Card>
        ) : (
          answers.map((answer) => <Tile key={answer.series} answer={answer} nowMs={nowMs} />)
        )}
      </div>
    </div>
  );
}

/** Exported for the switcher's own label line, so the sentence the issue asks
 * for ("12 days before Monaco") is checkable against every scenario. */
export const answerSentence = sentence;
