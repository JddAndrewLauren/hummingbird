import { Badge } from "../../components/core/Badge";
import { Card } from "../../components/core/Card";
import { EmptyState } from "../../components/feedback/EmptyState";
import type { QuestionInputs } from "../questions/contract";
import { NEVER_POLLED_SUBJECT, githubBand, githubGapReason, githubView } from "./github";

// The GitHub workflow pane's own expanded rendering — `KimiPaneExpanded`'s
// shape, carried over: a headline first, supporting detail below it,
// freshness last. There is no dormant rendering here, on the same pane's
// own reasoning — dormant IS the collapsed row, which the shell owns. A
// `distant` (cadence-unreadable) pane is different: it is non-`dormant`, so
// `collapse.ts`'s default rule opens it, and this card is the view the
// reader actually gets — it must carry the same "cadence unreadable" fact
// the collapsed headline does, or the unjudged overdue-ness reads as an
// ordinary healthy card (#314 review round 2).

function ageLabel(ageMs: number): string {
  const hours = Math.floor(ageMs / 3_600_000);
  if (hours < 1) {
    return "under an hour ago";
  }
  if (hours < 48) {
    return `${hours}h ago`;
  }
  return `${Math.floor(hours / 24)}d ago`;
}

export function GithubPaneExpanded({
  subjectKey,
  inputs,
}: {
  subjectKey: string;
  inputs: QuestionInputs;
}) {
  if (subjectKey === NEVER_POLLED_SUBJECT) {
    return (
      <Card padding="var(--space-3)">
        <EmptyState
          compact
          icon="cloud-fog"
          headingLevel={3}
          title="No answer yet"
          body="Nothing has polled this yet."
        />
      </Card>
    );
  }

  const view = githubView(subjectKey, inputs);
  if (view === null) {
    return (
      <Card padding="var(--space-3)">
        <EmptyState
          compact
          icon="cloud-fog"
          headingLevel={3}
          title="No answer yet"
          body={githubGapReason(subjectKey, inputs)}
        />
      </Card>
    );
  }

  const { body, freshness, stale } = view;
  const band = githubBand(body, inputs.nowMs);
  const ageMs = freshness.kind === "age" ? freshness.ageMs : null;

  return (
    <Card
      padding="var(--space-5)"
      style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}
    >
      <p
        style={{
          font: "var(--weight-bold) 20px/1.1 var(--font-display)",
          letterSpacing: "var(--tracking-heading)",
          color:
            band === "live" || band === "imminent"
              ? "var(--status-danger-fg)"
              : band === "near" || band === "distant"
                ? "var(--status-warn-fg)"
                : "var(--text-primary)",
        }}
      >
        {body.displayName}
      </p>

      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: "var(--space-4)",
          flexWrap: "wrap",
        }}
      >
        <span className="hb-meta">
          {body.lastRunAtMs === null
            ? "never run"
            : `last run ${body.lastRunConclusion ?? "in progress"} (${body.lastRunEvent}), ${ageLabel(inputs.nowMs - body.lastRunAtMs)}`}
        </span>
        <span className="hb-meta">
          {body.lastScheduledSuccessAtMs === null
            ? "no scheduled success on record"
            : `last scheduled success ${ageLabel(inputs.nowMs - body.lastScheduledSuccessAtMs)}`}
        </span>
        {band === "live" || band === "imminent" ? (
          <Badge tone="danger" mono>
            cron stalled
          </Badge>
        ) : body.declaredCadenceMs === null ? (
          // The overdue judgement could not be made — `githubBand` bands
          // this `distant`, and this card must say so, not present two
          // green facts unwarned.
          <Badge tone="warn" mono>
            cadence unreadable
          </Badge>
        ) : null}
      </div>

      {stale ? (
        <span className="hb-meta" style={{ color: "var(--status-warn-fg)" }}>
          {ageMs === null ? "stale — age unknown" : `stale — as of ${Math.floor(ageMs / 3_600_000)}h ago`}
        </span>
      ) : null}
    </Card>
  );
}
