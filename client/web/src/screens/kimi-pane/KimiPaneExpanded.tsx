import { Badge } from "../../components/core/Badge";
import { Card } from "../../components/core/Card";
import { EmptyState } from "../../components/feedback/EmptyState";
import type { QuestionInputs } from "../questions/contract";
import { formatUsd, kimiBand, kimiGapReason, kimiView } from "./kimi";

// The Kimi balance pane's own expanded rendering — the one part of this
// question the shell does not draw. `WastePaneExpanded`'s shape, carried
// over: a headline number first, the supporting detail (here, the
// voucher/cash split) below it, freshness last.
//
// There is no dormant rendering here, on `WastePaneExpanded`'s own reasoning
// — dormant IS the collapsed row, which the shell owns.

export function KimiPaneExpanded({ inputs }: { subjectKey: string; inputs: QuestionInputs }) {
  const view = kimiView(inputs);
  if (view === null) {
    // Visibly broken, never quietly empty — the reason is words, on screen.
    // There is no setup prompt here (unlike `WastePaneExpanded`'s unbound
    // arm): this question has no per-device binding to point Settings at,
    // so "never polled yet" is the whole story.
    return (
      <Card padding="var(--space-3)">
        <EmptyState
          compact
          icon="cloud-fog"
          headingLevel={3}
          title="No balance answer yet"
          body={kimiGapReason(inputs)}
        />
      </Card>
    );
  }

  const { body, freshness, stale } = view;
  const band = kimiBand(body.availableBalance);
  const ageMs = freshness.kind === "age" ? freshness.ageMs : null;
  const cashNegative = body.cashBalance < 0;

  return (
    <Card
      padding="var(--space-5)"
      style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}
    >
      <p
        style={{
          font: "var(--weight-bold) 24px/1.1 var(--font-display)",
          letterSpacing: "var(--tracking-heading)",
          color:
            band === "live" || band === "imminent"
              ? "var(--status-danger-fg)"
              : band === "near"
                ? "var(--status-warn-fg)"
                : "var(--text-primary)",
        }}
      >
        {formatUsd(body.availableBalance)} left
      </p>

      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: "var(--space-4)",
          flexWrap: "wrap",
        }}
      >
        <span className="hb-meta">voucher {formatUsd(body.voucherBalance)}</span>
        <span
          className="hb-meta"
          style={cashNegative ? { color: "var(--status-danger-fg)" } : undefined}
        >
          cash {formatUsd(body.cashBalance)}
        </span>
        {/* The fact the ADR names explicitly: a positive `available_balance`
            can hide a negative cash position — the account owes, even while
            the headline number is still positive. */}
        {cashNegative ? (
          <Badge tone="danger" mono>
            cash owed
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
