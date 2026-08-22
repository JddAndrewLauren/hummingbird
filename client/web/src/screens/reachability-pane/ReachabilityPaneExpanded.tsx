import { Card } from "../../components/core/Card";
import { PaneGap } from "../questions/PaneGap";
import type { QuestionInputs } from "../questions/contract";
import { reachabilityView } from "./reachability";

/** The pane's content, with no `Card` of its own — what the Status board's
 * expanded tile draws inside the one card it already is. `*PaneExpanded`
 * below is this plus Now's card. */
/** Whether this body draws its own headline.
 *
 * Now's card does (it is the pane's whole rendering there). The Status
 * board's expanded tile does not: that tile's header row already carries the
 * pane's decided `collapsedHeadline`, so a body that drew its own would say
 * the same thing twice, two lines apart. */
export function ReachabilityPaneBody({
  inputs,
  headline = true,
}: {
  subjectKey?: string;
  inputs: QuestionInputs;
  headline?: boolean;
}) {
  const view = reachabilityView(inputs);
  if (view === null) {
    return (
      <PaneGap
        headline={headline}
        title="Never synced on this device."
        body="No successful authority sync is recorded for this device."
      />
    );
  }

  // This pane's whole answer IS its headline — there is no supporting
  // detail under it. So when the host draws the headline itself, this body
  // has nothing left to say, and says nothing rather than repeating it.
  if (!headline) {
    return null;
  }

  return (
    <p
      style={{
        font: "var(--weight-bold) 20px/1.1 var(--font-display)",
        letterSpacing: "var(--tracking-heading)",
        color: view.stale ? "var(--status-danger-fg)" : "var(--text-primary)",
      }}
    >
      {view.headline}
    </p>
  );
}

export function ReachabilityPaneExpanded({
  inputs,
}: {
  subjectKey: string;
  inputs: QuestionInputs;
}) {
  // The gap arm sits tighter than the answered one, exactly as it did when
  // this component held both trees itself. Re-reading the (pure) view to
  // pick the padding costs a call and keeps the two arms' chrome unchanged.
  const gap = reachabilityView(inputs) === null;
  return (
    <Card padding={gap ? "var(--space-3)" : "var(--space-5)"}>
      <ReachabilityPaneBody inputs={inputs} />
    </Card>
  );
}
