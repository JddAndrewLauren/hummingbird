import { Card } from "../../components/core/Card";
import { PaneGap } from "../questions/PaneGap";
import type { QuestionInputs } from "../questions/contract";
import { ageWords, cadenceWords, pollerGapReason, pollerView } from "./poller";

// The poller pane's own expanded rendering — `UptimePaneExpanded`'s shape,
// carried over: a headline first, the raw freshness below it. There is no
// dormant rendering here either, on the same reasoning — dormant IS the
// collapsed row, which the shell owns.

/** The pane's content, with no `Card` of its own — what the Status board's
 * expanded tile draws inside the one card it already is. */
export function PollerPaneBody({
  subjectKey,
  inputs,
  headline = true,
}: {
  subjectKey: string;
  inputs: QuestionInputs;
  headline?: boolean;
}) {
  const view = pollerView(subjectKey, inputs);
  if (view === null) {
    return (
      <PaneGap headline={headline} title="No answer yet" body={pollerGapReason(subjectKey, inputs)} />
    );
  }

  const { source, band, freshness } = view;
  const ageMs = freshness.kind === "age" ? freshness.ageMs : null;
  const declaredCadenceMs = freshness.kind === "age" ? freshness.declaredCadenceMs : null;

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: "var(--space-4)",
      }}
    >
      {headline ? (
        <p
          style={{
            font: "var(--weight-bold) 20px/1.1 var(--font-display)",
            letterSpacing: "var(--tracking-heading)",
            color:
              band === "imminent"
                ? "var(--status-danger-fg)"
                : band === "distant"
                  ? "var(--status-warn-fg)"
                  : "var(--text-primary)",
          }}
        >
          {source}
        </p>
      ) : null}

      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: "var(--space-4)",
          flexWrap: "wrap",
        }}
      >
        <span className="hb-meta">{ageMs === null ? "age unknown" : `as of ${ageWords(ageMs)}`}</span>
        <span className="hb-meta">
          {declaredCadenceMs === null
            ? "cadence unreadable"
            : `declared cadence ${cadenceWords(declaredCadenceMs)}`}
        </span>
      </div>
    </div>
  );
}

export function PollerPaneExpanded({
  subjectKey,
  inputs,
}: {
  subjectKey: string;
  inputs: QuestionInputs;
}) {
  const gap = pollerView(subjectKey, inputs) === null;
  return (
    <Card padding={gap ? "var(--space-3)" : "var(--space-5)"}>
      <PollerPaneBody subjectKey={subjectKey} inputs={inputs} />
    </Card>
  );
}
