import { Badge } from "../../components/core/Badge";
import { Button } from "../../components/core/Button";
import { Card } from "../../components/core/Card";
import { Icon } from "../../components/core/Icon";
import { EmptyState } from "../../components/feedback/EmptyState";
import type { QuestionInputs } from "../questions/contract";
import {
  clock,
  dayLabel,
  raceGapReason,
  raceHeadline,
  raceSetup,
  raceView,
  seriesLabel,
} from "./race";

// The next-race pane's own expanded rendering — the only part of this
// question the shell does not draw. It is the prototype's winning "series
// tile" rewritten under real rules (`prototype-race-pane/NOTES.md`'s fold-in
// checklist), with two of its verdicts deliberately reversed by decisions
// taken after it: **no zone label** (ADR-0015 is device-local; the hardcoded
// Pacific and its "PT" suffix are gone) and **no "under way" headline** (the
// feed publishes no session end time and #266 refused to invent one — see
// `race.ts`'s `nextRaceAt`).
//
// There is no dormant or collapsed rendering here: the shell owns the
// collapsed row, and off-season simply changes the words.

export function RacePaneExpanded({
  subjectKey,
  inputs,
  onSetupNavigate,
}: {
  subjectKey: string;
  inputs: QuestionInputs;
  onSetupNavigate?: () => void;
}) {
  const setup = raceSetup(inputs);
  if (setup.kind === "unread") {
    // The bindings table has not answered yet. Not the setup prompt: telling
    // someone who already follows a series to set one up is a wrong answer,
    // not a slow one.
    return (
      <Card padding="var(--space-3)">
        <EmptyState
          compact
          icon="cloud-fog"
          headingLevel={3}
          title="Checking your setup"
          body="Reading this device's settings."
        />
      </Card>
    );
  }
  if (setup.kind !== "bound") {
    const unusable = setup.kind === "unusable";
    return (
      <Card padding="var(--space-3)">
        <EmptyState
          compact
          icon="help-circle"
          headingLevel={3}
          title={unusable ? "That series list can't be read" : "Which series do you follow?"}
          body={
            unusable
              ? "The race series setting holds something that isn't text. Set it again."
              : "Name the racing series to follow, separated by commas."
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

  const view = raceView(subjectKey, inputs);
  if (view === null) {
    // Visibly broken, never quietly empty — including the ordinary case of a
    // followed series with no adapter upstream (#266 ships F1 only).
    return (
      <Card padding="var(--space-3)">
        <EmptyState
          compact
          icon="cloud-fog"
          headingLevel={3}
          title={`No ${seriesLabel(subjectKey)} schedule yet`}
          body={raceGapReason(subjectKey, inputs)}
        />
      </Card>
    );
  }

  const live = view.liveAlert !== null;
  const ageMs = view.freshness.kind === "age" ? view.freshness.ageMs : null;

  return (
    <Card
      accent={live}
      padding="var(--space-5)"
      style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}
    >
      <span
        className="hb-meta"
        style={{ display: "inline-flex", alignItems: "center", gap: "var(--space-3)" }}
      >
        <Icon name={live ? "siren" : "flag"} size={13} />
        {view.label}
      </span>

      <p
        style={{
          font: "var(--weight-bold) 24px/1.1 var(--font-display)",
          letterSpacing: "var(--tracking-heading)",
          color: "var(--text-primary)",
        }}
      >
        {raceHeadline(view, inputs.nowMs)}
      </p>

      {view.event !== null && view.nextStart !== null ? (
        <>
          {/* The headline counts to race day; this line carries the thing
              that actually happens first, which for most of a race weekend
              is Friday practice. Once the ladder is done it is the race
              itself, so the line is never absent. */}
          <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
            {view.nextStart.label} · {dayLabel(view.nextStart.startsAtMs, inputs.nowMs)}{" "}
            {clock(view.nextStart.startsAtMs)}
          </p>
          {/* The circuit, and only the circuit: the headline already names
              the event, and repeating it here says the same thing twice. */}
          <p style={{ font: "var(--type-body-sm)", color: "var(--text-muted)" }}>
            {view.event.locality}
          </p>
        </>
      ) : null}

      {view.liveAlert !== null ? (
        <div style={{ display: "flex", alignItems: "center", gap: "var(--space-3)", flexWrap: "wrap" }}>
          <Badge tone="danger" icon="siren" mono>
            starting soon
          </Badge>
          <span style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
            {view.liveAlert.title}
          </span>
        </div>
      ) : null}

      {/* Kept showing, and its age said out loud — the brand's own rule. An
          `unknown` age has no hours to name, so it says that instead of
          fabricating a number. */}
      {view.stale ? (
        <span className="hb-meta" style={{ color: "var(--status-warn-fg)" }}>
          {ageMs === null
            ? "stale — age unknown"
            : `stale — as of ${Math.floor(ageMs / 3_600_000)}h ago`}
        </span>
      ) : null}
    </Card>
  );
}
