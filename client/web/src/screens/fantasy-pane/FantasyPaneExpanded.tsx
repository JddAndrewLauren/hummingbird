import { Badge } from "../../components/core/Badge";
import { Button } from "../../components/core/Button";
import { Card } from "../../components/core/Card";
import { Icon } from "../../components/core/Icon";
import { EmptyState } from "../../components/feedback/EmptyState";
import type { LineupFactsCore, LineupSlotCore } from "../../decisions/seam";
import type { QuestionInputs } from "../questions/contract";
import {
  designationWords,
  fantasyGapReason,
  fantasySetup,
  leagueLabel,
  lineupFacts,
  lineupHeadline,
  lockTime,
  parseSubjectKey,
  slotProblemWords,
  subjectLabel,
} from "./fantasy";

// The fantasy pane's expanded rendering. Unbound, it is the setup prompt;
// bound but unpolled, it names the league and the obligation and says
// plainly that nothing has been polled (#717). An answered lineup (#855) is
// the core's headline again, then every starting slot: who fills it, and —
// for an invalid one — why, and until when it can be fixed. The waivers
// subject's answered rendering arrives with #719.

export function FantasyPaneExpanded({
  subjectKey,
  inputs,
  onSetupNavigate,
}: {
  subjectKey: string;
  inputs: QuestionInputs;
  onSetupNavigate?: () => void;
}) {
  const setup = fantasySetup(inputs);
  const parsed = parseSubjectKey(subjectKey);
  const subject = subjectLabel(parsed?.subject ?? "lineup");

  if (setup.kind === "unread") {
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
          title={unusable ? "That league list can't be read" : "Which leagues do you play in?"}
          body={
            unusable
              ? "The Yahoo leagues setting holds something that isn't text. Set it again."
              : "Name your Yahoo league keys, separated by commas."
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

  const facts = parsed?.subject === "lineup" ? lineupFacts(subjectKey, inputs) : null;
  if (facts !== null) {
    return <LineupCard facts={facts} nowMs={inputs.nowMs} />;
  }

  const league = parsed === null ? subjectKey : leagueLabel(parsed.league);
  return (
    <Card padding="var(--space-3)">
      <EmptyState
        compact
        icon="cloud-fog"
        headingLevel={3}
        title={`${league} · ${subject}`}
        body={fantasyGapReason(subjectKey, inputs)}
      />
    </Card>
  );
}

function LineupCard({ facts, nowMs }: { facts: LineupFactsCore; nowMs: number }) {
  const ageMs = facts.freshness.kind === "age" ? facts.freshness.ageMs : null;
  const invalid = facts.headline.kind === "invalid";
  return (
    <Card
      padding="var(--space-5)"
      style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}
    >
      <span className="hb-meta" style={{ display: "inline-flex", alignItems: "center", gap: "var(--space-3)" }}>
        <Icon name="list-checks" size={13} />
        {facts.leagueName} · Week {facts.week}
      </span>
      <p
        style={{
          font: "var(--type-body-strong)",
          color: invalid ? "var(--status-warn-fg)" : "var(--text-primary)",
          overflowWrap: "anywhere",
        }}
      >
        {lineupHeadline(facts.headline, nowMs)}
      </p>
      <ul aria-label={`${facts.teamName} starting slots`} style={{ listStyle: "none", margin: 0, padding: 0 }}>
        {facts.slots.map((slot, index) => (
          <SlotRow key={`${slot.position}-${index}`} slot={slot} nowMs={nowMs} />
        ))}
      </ul>
      {/* Kept showing, and its age said out loud — the brand's own rule, and
          the race pane's wording. */}
      {facts.stale ? (
        <span className="hb-meta" style={{ color: "var(--status-warn-fg)" }}>
          {ageMs === null ? "stale — age unknown" : `stale — as of ${Math.floor(ageMs / 3_600_000)}h ago`}
        </span>
      ) : null}
    </Card>
  );
}

/** One starting slot: its position, its occupant (or that it is empty) and
 * any designation; an invalid slot adds why, and its replacement deadline. */
function SlotRow({ slot, nowMs }: { slot: LineupSlotCore; nowMs: number }) {
  const deadline = slot.invalid?.deadlineMs ?? null;
  return (
    <li
      style={{
        display: "grid",
        gridTemplateColumns: "3.5rem minmax(0, 1fr)",
        columnGap: "var(--space-3)",
        rowGap: "var(--space-1)",
        padding: "var(--space-2) 0",
        borderTop: "1px solid var(--border-subtle)",
      }}
    >
      <span className="hb-meta">{slot.position}</span>
      <span
        style={{
          font: "var(--type-body-sm)",
          color: slot.occupant === null ? "var(--text-muted)" : "var(--text-primary)",
          overflowWrap: "anywhere",
        }}
      >
        {slot.occupant === null ? "No starter" : slot.occupant.name}
        {slot.occupant?.designation && slot.invalid === null ? (
          <span style={{ color: "var(--text-muted)" }}> · {designationWords(slot.occupant.designation)}</span>
        ) : null}
      </span>
      {slot.invalid !== null ? (
        <span
          style={{
            gridColumn: 2,
            display: "flex",
            alignItems: "center",
            gap: "var(--space-3)",
            flexWrap: "wrap",
          }}
        >
          <Badge tone="warn" mono>
            {slotProblemWords(slot.invalid.reason)}
          </Badge>
          <span style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
            {deadline === null ? "No replacement deadline" : `Fix by ${lockTime(deadline, nowMs)}`}
          </span>
        </span>
      ) : null}
    </li>
  );
}
