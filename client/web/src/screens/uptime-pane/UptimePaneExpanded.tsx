import { Badge } from "../../components/core/Badge";
import { Card } from "../../components/core/Card";
import { EmptyState } from "../../components/feedback/EmptyState";
import type { QuestionInputs } from "../questions/contract";
import { NEVER_POLLED_SUBJECT, uptimeBand, uptimeGapReason, uptimeView } from "./uptime";

// The uptime pane's own expanded rendering — `GithubPaneExpanded`'s shape,
// carried over: a headline first, the raw observation below it, freshness
// last. There is no dormant rendering here, on the same pane's own
// reasoning — dormant IS the collapsed row, which the shell owns. Every
// non-dormant band opens on its own (`collapse.ts`'s default rule), and
// this card must carry the same fact its collapsed headline does — an
// unreachable host's own error text, a wrong status code, or an
// unexpectedly-answering service that should be off — never a silently
// green card behind a red collapsed row.

export function UptimePaneExpanded({
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

  const view = uptimeView(subjectKey, inputs);
  if (view === null) {
    return (
      <Card padding="var(--space-3)">
        <EmptyState
          compact
          icon="cloud-fog"
          headingLevel={3}
          title="No answer yet"
          body={uptimeGapReason(subjectKey, inputs)}
        />
      </Card>
    );
  }

  const { serviceId, body, freshness, stale } = view;
  const band = uptimeBand(body);
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
            band === "live"
              ? "var(--status-danger-fg)"
              : band === "near"
                ? "var(--status-warn-fg)"
                : "var(--text-primary)",
        }}
      >
        {serviceId}
      </p>

      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: "var(--space-4)",
          flexWrap: "wrap",
        }}
      >
        <span className="hb-meta">expected {body.expected}</span>
        <span className="hb-meta">
          {body.error !== null
            ? `unreachable — ${body.error}`
            : `answered ${body.observedStatus} (wanted ${body.expectStatus})`}
        </span>
        {body.expected === "off" && body.error === null ? (
          <Badge tone="danger" mono>
            reachable when it should be off
          </Badge>
        ) : body.expected === "on" && body.error !== null ? (
          <Badge tone="danger" mono>
            unreachable
          </Badge>
        ) : body.expected === "on" && body.observedStatus !== body.expectStatus ? (
          <Badge tone="warn" mono>
            unexpected status
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
