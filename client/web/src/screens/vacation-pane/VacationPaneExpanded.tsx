import type { CSSProperties } from "react";
import { Badge } from "../../components/core/Badge";
import { Button } from "../../components/core/Button";
import { Card } from "../../components/core/Card";
import { EmptyState } from "../../components/feedback/EmptyState";
import type { QuestionInputs } from "../questions/contract";
import {
  HORIZON_LABEL,
  tripDateRange,
  tripDayLabel,
  vacationHeadline,
  vacationSetup,
  vacationView,
  type Trip,
} from "./vacation";

// The vacation countdown's own expanded rendering — the only part of this
// question the shell does not draw. Variant A's settled verdict, unchanged:
// place-first, place and count at the same display size with the joining
// words between them at body size, the whole trip queue listed and never
// truncated, muted mono rows (name left, date right, so the dates form a
// column the eye can run down), no "then" on the first row, and a year on any
// date outside the current one.
//
// There is no dormant rendering here. Dormant IS the collapsed row, which the
// shell owns.

const META: CSSProperties = {
  font: "var(--type-meta)",
  letterSpacing: "var(--tracking-meta)",
  textTransform: "uppercase",
};

/** The two things the headline says — the place and the count — set
 * identically, so neither reads as a caption on the other. */
const HEADLINE: CSSProperties = {
  font: "var(--weight-bold) 34px/1 var(--font-display)",
  letterSpacing: "var(--tracking-display)",
  color: "var(--text-primary)",
};

/** The words between them, which are grammar rather than answer. */
const JOIN: CSSProperties = {
  font: "var(--type-body)",
  color: "var(--text-secondary)",
};

function Countdown({ trip }: { trip: Trip }) {
  if (trip.phase !== "upcoming" || trip.daysUntil === 1) {
    // Every phase but a plain multi-day countdown is a sentence, not two
    // facts with a hinge between them — "In Lisbon · day 3 of 6" has no
    // number to set apart.
    return (
      <p
        style={{
          font: "var(--weight-bold) 20px/1.15 var(--font-display)",
          letterSpacing: "var(--tracking-heading)",
          color: "var(--text-primary)",
        }}
      >
        {vacationHeadline(trip)}
      </p>
    );
  }
  return (
    <p
      style={{ display: "flex", alignItems: "baseline", flexWrap: "wrap", gap: "var(--space-3)" }}
    >
      <span style={HEADLINE}>{trip.name}</span>
      <span style={JOIN}>in</span>
      <span style={{ ...HEADLINE, fontVariantNumeric: "tabular-nums" }}>{trip.daysUntil}</span>
      <span style={JOIN}>days</span>
    </p>
  );
}

export function VacationPaneExpanded({
  inputs,
  onSetupNavigate,
}: {
  subjectKey: string;
  inputs: QuestionInputs;
  onSetupNavigate?: () => void;
}) {
  const setup = vacationSetup(inputs);

  if (setup.kind === "no-calendar" || setup.kind === "unbound") {
    // Two different missing steps, two different asks — collapsing them
    // would tell someone with a connected calendar to connect one.
    const noCalendar = setup.kind === "no-calendar";
    return (
      <Card padding="var(--space-3)">
        <EmptyState
          compact
          icon="help-circle"
          headingLevel={3}
          title="How long to the next vacation?"
          body={
            noCalendar
              ? "Connect a calendar, then designate the one your trips live on."
              : "Designate a Trips calendar — the countdown reads its events."
          }
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

  if (setup.kind === "unread") {
    return (
      <Card padding="var(--space-3)">
        <EmptyState
          compact
          icon="cloud-fog"
          headingLevel={3}
          title="Waiting for the first calendar sync"
          body="Nothing to count to until this device has read the Trips calendar."
        />
      </Card>
    );
  }

  const view = vacationView(inputs);
  if (view === null) {
    return null;
  }
  const { next, later, stale, freshness } = view;
  const ageMs = freshness.kind === "age" ? freshness.ageMs : null;

  return (
    <Card
      accent={next !== null && next.phase !== "upcoming"}
      padding="var(--space-5)"
      style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}
    >
      {next === null ? (
        <p style={{ font: "var(--type-body)", color: "var(--text-muted)" }}>
          {`Nothing booked in the next ${HORIZON_LABEL}.`}
        </p>
      ) : (
        <>
          <Countdown trip={next} />
          <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
            {tripDateRange(next, inputs.nowMs)} · {next.lengthDays} days
          </p>
          {next.location ? (
            <p style={{ font: "var(--type-body-sm)", color: "var(--text-muted)" }}>
              {next.location}
            </p>
          ) : null}
          {next.phase === "under_way" ? (
            // Under way, the count that matters is the one running out. A
            // badge repeating the headline would be two of the same fact.
            <div>
              <Badge tone="brand" icon="calendar" mono>
                day {next.dayOfTrip} of {next.lengthDays}
              </Badge>
            </div>
          ) : null}
        </>
      )}

      {later.length > 0 ? (
        <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
          {later.map((trip) => (
            <p
              key={trip.id}
              style={{
                ...META,
                display: "flex",
                justifyContent: "space-between",
                gap: "var(--space-4)",
                color: "var(--text-muted)",
              }}
            >
              <span>{trip.name}</span>
              <span style={{ whiteSpace: "nowrap" }}>{tripDayLabel(trip, inputs.nowMs)}</span>
            </p>
          ))}
        </div>
      ) : null}

      {/* Stale never suppresses the answer — the countdown above still
          renders, with its age said out loud beneath it. An `unknown` age has
          no hours to name, so it says that rather than fabricating a number. */}
      {stale ? (
        <p style={{ ...META, marginTop: "var(--space-2)", color: "var(--status-warn-fg)" }}>
          {ageMs === null
            ? "Stale — age unknown"
            : `Stale — as of ${Math.floor(ageMs / 3_600_000)}h ago`}
        </p>
      ) : null}
    </Card>
  );
}
