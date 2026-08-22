import { Card } from "../../components/core/Card";
import { Icon } from "../../components/core/Icon";
import { EmptyState } from "../../components/feedback/EmptyState";
import { FitText } from "../questions/FitText";
import type { QuestionInputs } from "../questions/contract";
import {
  scpsCardTitle,
  scpsDayLabel,
  scpsQuestLine,
  scpsTimeLabel,
  scpsView,
  type ScpsEvent,
} from "./scps";

// The SCPS pane's own expanded rendering (#693, ADR-0032) — the next event
// (kind, time, location, notes), the Photo Quest, and any further `SCPS `
// events still in the window.
//
// **The meta line owns the day and time; the title never repeats them.**
// `scpsHeadline` is the collapsed row's whole sentence (kind, day, time,
// topic in one line, because the row has no second line); rendering it here
// above a `day · time` meta line would state the same fact twice —
// `VacationPaneExpanded.tsx`'s "a badge repeating the headline would be two
// of the same fact". So this card titles with `scpsCardTitle` (kind and
// topic only) and says the day and time exactly once, in the meta line.
//
// **Plain title + meta lines, never `ItemRow`.** The aside's 320px
// ellipsises a title-plus-chips row down to nothing (memory: "ItemRow
// ellipsises in the 320px aside", the visual gate's own finding on
// `homework-pane`) — this pane's further-events list is a title line and a
// day label, on `HomeworkPaneExpanded.tsx`'s `OtherItem` precedent, not a
// row built for the frontier's own chips.
//
// **Nothing here decides.** The band, the queue, the quest classification
// and the month-token parse all arrive from `scps.rs` through `scps.ts`;
// this file only composes sentences off already-decided facts.

function LaterEvent({ event }: { event: ScpsEvent }) {
  return (
    <div style={{ display: "flex", gap: "var(--space-3)", alignItems: "baseline" }}>
      <Icon name="calendar" size={13} style={{ position: "relative", top: 2 }} />
      <span style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
        {scpsCardTitle(event)}
        <span className="hb-meta" style={{ marginLeft: "var(--space-3)" }}>
          {scpsDayLabel(event)}
        </span>
      </span>
    </div>
  );
}

export function ScpsPaneExpanded({ inputs }: { subjectKey: string; inputs: QuestionInputs }) {
  const view = scpsView(inputs);

  if (view === null) {
    return (
      <Card padding="var(--space-3)">
        <EmptyState
          compact
          icon="cloud-fog"
          headingLevel={3}
          title="Waiting for the first calendar sync"
          body="Nothing to show until this device has read its calendars."
        />
      </Card>
    );
  }

  const { next, later, quest, stale, freshness } = view;
  const ageMs = freshness.kind === "age" ? freshness.ageMs : null;

  return (
    <Card
      accent={next !== null && (next.inProgress || next.daysUntil <= 0)}
      padding="var(--space-5)"
      style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}
    >
      {next === null ? (
        <p style={{ font: "var(--type-body)", color: "var(--text-muted)" }}>
          No SCPS event scheduled.
        </p>
      ) : (
        <>
          <FitText
            basePx={26}
            style={{
              fontFamily: "var(--font-display)",
              fontWeight: "var(--weight-bold)",
              lineHeight: 1.15,
              letterSpacing: "var(--tracking-heading)",
              color: "var(--text-primary)",
            }}
          >
            {scpsCardTitle(next)}
          </FitText>
          <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
            {scpsDayLabel(next)} · {scpsTimeLabel(next.startMs)}
          </p>
          {next.location !== null ? (
            <p style={{ font: "var(--type-body-sm)", color: "var(--text-muted)" }}>
              {next.location}
            </p>
          ) : null}
          {next.notes !== null && next.notes.trim().length > 0 ? (
            <p style={{ font: "var(--type-body-sm)", color: "var(--text-muted)" }}>
              {next.notes}
            </p>
          ) : null}
        </>
      )}

      <p className="hb-meta" style={{ color: "var(--text-muted)" }}>
        {scpsQuestLine(quest, inputs.nowMs)}
      </p>

      {later.length > 0 ? (
        <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
          {later.map((event) => (
            <LaterEvent key={event.id} event={event} />
          ))}
        </div>
      ) : null}

      {stale ? (
        <p className="hb-meta" style={{ marginTop: "var(--space-2)", color: "var(--status-warn-fg)" }}>
          {ageMs === null
            ? "Stale — age unknown"
            : `Stale — as of ${Math.floor(ageMs / 3_600_000)}h ago`}
        </p>
      ) : null}
    </Card>
  );
}
