import { Card } from "../../components/core/Card";
import { EmptyState } from "../../components/feedback/EmptyState";
import type { QuestionInputs } from "../questions/contract";
import { reachabilityView } from "./reachability";

export function ReachabilityPaneExpanded({ inputs }: { subjectKey: string; inputs: QuestionInputs }) {
  const view = reachabilityView(inputs);
  if (view === null) {
    return (
      <Card padding="var(--space-3)">
        <EmptyState
          compact
          icon="cloud-fog"
          headingLevel={3}
          title="Never synced on this device."
          body="No successful authority sync is recorded for this device."
        />
      </Card>
    );
  }

  return (
    <Card padding="var(--space-5)">
      <p
        style={{
          font: "var(--weight-bold) 20px/1.1 var(--font-display)",
          letterSpacing: "var(--tracking-heading)",
          color: view.stale ? "var(--status-danger-fg)" : "var(--text-primary)",
        }}
      >
        {view.headline}
      </p>
    </Card>
  );
}
