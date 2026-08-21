import { Card } from "../../components/core/Card";
import { ItemRow } from "../../components/domain/ItemRow";
import { EmptyState } from "../../components/feedback/EmptyState";
import type { QuestionInputs } from "../questions/contract";
import { computeUrgency } from "../urgency";
import { homeworkHeadline, homeworkView } from "./homework";

// The homework pane's own expanded rendering (#675) — the winning item, the
// notes written on it, and whatever else is still open beneath.
//
// **Read-only, deliberately.** Every affordance this pane could grow (mark
// done, open the item, edit the notes) already exists on the frontier and
// on the item detail panel; adding a second one here would be a second
// write path onto the same item for no reason but proximity, and #675's own
// decision table is flat that the body is a read. The pane's job is that
// the notes are *reachable* without hunting for the item, which a rendering
// alone satisfies.
//
// **Nothing here decides.** The band, the answer state and `daysAway`
// arrive from `homework.rs` through `homework.ts`; the words are
// `homeworkHeadline`'s, reused rather than rewritten so the collapsed row
// and this card can never say different things about the same item.

export function HomeworkPaneExpanded({ inputs }: { subjectKey: string; inputs: QuestionInputs }) {
  const facts = homeworkView(inputs);
  if (facts === null) {
    // Visibly broken, never quietly empty — and it says which thing is
    // missing rather than showing an empty list that reads as "nothing due".
    return (
      <Card padding="var(--space-3)">
        <EmptyState
          compact
          icon="cloud-fog"
          headingLevel={3}
          title="Can't read this device's time zone"
          body="Without it there is no way to say which day a deadline falls on."
        />
      </Card>
    );
  }

  if (facts.winner === null) {
    // An empty homework list is good news, reported as a fact — the brand's
    // own rule about empty states.
    return (
      <Card padding="var(--space-3)">
        <EmptyState
          compact
          icon="circle-check"
          headingLevel={3}
          title="No open homework"
          body="Capture one with the @homework context and it shows up here."
        />
      </Card>
    );
  }

  const winner = facts.winner;
  return (
    <Card
      padding="var(--space-5)"
      style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}
    >
      <span className="hb-meta">{homeworkHeadline(facts)}</span>

      <ItemRow
        title={winner.title}
        deadline={winner.deadline ?? undefined}
        urgency={computeUrgency(winner.deadline, inputs.nowMs)}
      />

      {/* The whole point of the pane: the preparation notes, in the reader's
          own words, without going to find the item. `pre-wrap` because they
          were typed with their own line breaks and this is the one place
          they are read. */}
      {winner.description === null ? null : (
        <p
          style={{
            font: "var(--type-body-sm)",
            color: "var(--text-secondary)",
            whiteSpace: "pre-wrap",
            margin: 0,
          }}
        >
          {winner.description}
        </p>
      )}

      {facts.others.length === 0 ? null : (
        <>
          <span className="hb-meta" style={{ color: "var(--text-muted)" }}>
            {facts.others.length === 1 ? "1 more open" : `${facts.others.length} more open`}
          </span>
          {facts.others.map((item) => (
            <ItemRow
              key={item.id}
              title={item.title}
              deadline={item.deadline ?? undefined}
              urgency={computeUrgency(item.deadline, inputs.nowMs)}
            />
          ))}
        </>
      )}
    </Card>
  );
}
