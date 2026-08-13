import { Card } from "../../components/core/Card";
import { EmptyState } from "../../components/feedback/EmptyState";
import type { QuestionDef } from "./contract";

// ADR-0017's four infra questions (#311's slice) start life as this one
// shape: never polled, so every one of them is `bound-but-unacquired` and
// renders the sentinel gap `subjects()`'s contract now documents for the
// never-polled case. #313-#316 replace each of these wholesale with a real
// poller-backed question — this factory exists only so the four agree on
// what "never polled" looks like while they wait, not to be a lasting
// abstraction those questions inherit from.
//
// `sources: []` on every one: a placeholder reads no `context_snapshots`
// lane, because none is enrolled yet (ADR-0017's decision 2 — the source
// string arrives with the poller, in #313/#314/#315; #316 never gets one at
// all). `calendarRequests` is left undeclared for the same reason.

/** One never-polled infra question, on the Status surface. `subjectKey` is
 * both the question's one sentinel subject and its collapsed row's glyph
 * label. */
export function placeholderQuestion(label: string, subjectKey: string): QuestionDef {
  function PlaceholderExpanded() {
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

  return {
    label,
    surface: "status",
    sources: [],
    subjects: () => [subjectKey],
    answer: () => ({
      answerState: "bound-but-unacquired",
      band: "dormant",
      withinBand: null,
      collapsedHeadline: "No answer yet",
      icon: [{ kind: "icon", name: "cloud-fog", label: "no answer yet" }],
    }),
    Expanded: PlaceholderExpanded,
  };
}
