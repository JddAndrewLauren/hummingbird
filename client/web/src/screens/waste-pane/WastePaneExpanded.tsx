import { Badge } from "../../components/core/Badge";
import { Button } from "../../components/core/Button";
import { Card } from "../../components/core/Card";
import { EmptyState } from "../../components/feedback/EmptyState";
import type { QuestionInputs } from "../questions/contract";
import { BIN, orderedStreams, wasteGapReason, wasteHeadline, wasteSetup, wasteView } from "./waste";
import type { Stream } from "./waste";

// The which-cans pane's own expanded rendering — the *only* part of this
// question the shell does not draw. Carried over from the prototype's
// "kerb tile" variant, whose settled verdict was that the bins do the
// talking: real bin colours, three words, a date.
//
// There is no dormant rendering here. Dormant IS the collapsed row, which
// the shell owns — a pane that shipped its own quiet form would be a second
// place the region's furniture is defined.

function Bin({ stream }: { stream: Stream }) {
  const bin = BIN[stream];
  return (
    <span
      role="img"
      aria-label={bin.label}
      style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 3 }}
    >
      <span
        style={{ width: 34, height: 5, borderRadius: "var(--radius-sm)", background: bin.edge }}
      />
      <span
        style={{
          width: 30,
          height: 38,
          borderRadius: "var(--radius-sm)",
          border: `1px solid ${bin.edge}`,
          background: bin.fill,
        }}
      />
    </span>
  );
}

export function WastePaneExpanded({
  inputs,
  onSetupNavigate,
}: {
  subjectKey: string;
  inputs: QuestionInputs;
  onSetupNavigate?: () => void;
}) {
  const setup = wasteSetup(inputs);
  if (setup.kind === "unread") {
    // The bindings table has not answered yet. Not the setup prompt: this
    // device does not yet know whether the question was ever asked, and
    // saying "set this up" to someone who already did is a wrong answer, not
    // a slow one.
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
    // A question nobody has answered yet — or one whose answer this build
    // cannot use — is setup, and the honest thing to do with setup is offer
    // the one action that resolves it.
    const unusable = setup.kind === "unusable";
    return (
      <Card padding="var(--space-3)">
        <EmptyState
          compact
          icon="help-circle"
          headingLevel={3}
          title={unusable ? "That collection page can't be read" : "Which cans go out?"}
          body={
            unusable
              ? "The collection page setting holds something that isn't text. Set it again."
              : "Set the council page your collection schedule is read from."
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

  const view = wasteView(inputs);
  if (view === null) {
    // Visibly broken, never quietly empty: the reason is words, on screen.
    return (
      <Card padding="var(--space-3)">
        <EmptyState
          compact
          icon="cloud-fog"
          headingLevel={3}
          title="No collection answer yet"
          body={wasteGapReason(inputs)}
        />
      </Card>
    );
  }

  const ageMs = view.freshness.kind === "age" ? view.freshness.ageMs : null;

  return (
    <Card
      accent
      padding="var(--space-5)"
      style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}
    >
      <div style={{ display: "flex", gap: "var(--space-4)", justifyContent: "center" }}>
        {orderedStreams(view.body.streams).map((stream) => (
          <Bin key={stream} stream={stream} />
        ))}
      </div>

      <p
        style={{
          font: "var(--weight-bold) 24px/1.1 var(--font-display)",
          letterSpacing: "var(--tracking-heading)",
          color: "var(--text-primary)",
        }}
      >
        {wasteHeadline(view.daysAway, view.weekday, view.holiday)}
      </p>

      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: "var(--space-3)",
          flexWrap: "wrap",
        }}
      >
        <span className="hb-meta">
          {view.weekday.slice(0, 3)} {view.body.collectedOn}
        </span>
        {view.holiday ? (
          <Badge tone="warn" mono>
            holiday
          </Badge>
        ) : null}
        {/* Kept showing, and its age said out loud — the brand's own rule.
            An `unknown` age has no hours to name, so it says that instead of
            fabricating a number. */}
        {view.stale ? (
          <span
            className="hb-meta"
            style={{ color: "var(--status-warn-fg)" }}
          >
            {ageMs === null
              ? "stale — age unknown"
              : `stale — as of ${Math.floor(ageMs / 3_600_000)}h ago`}
          </span>
        ) : null}
      </div>
    </Card>
  );
}
