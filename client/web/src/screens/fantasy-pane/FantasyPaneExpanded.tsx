import { Button } from "../../components/core/Button";
import { Card } from "../../components/core/Card";
import { EmptyState } from "../../components/feedback/EmptyState";
import type { QuestionInputs } from "../questions/contract";
import {
  fantasyGapReason,
  fantasySetup,
  leagueLabel,
  parseSubjectKey,
  subjectLabel,
} from "./fantasy";

// The fantasy pane's expanded rendering (#717) — its two no-data arms only.
// Unbound, it is the setup prompt; bound, it names the league and the
// obligation and says plainly that nothing has been polled. The answered
// renderings arrive with the poller (#718) and the waiver deadline (#719).

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
