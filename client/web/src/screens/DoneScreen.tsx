import { Card } from "../components/core/Card";
import { Icon } from "../components/core/Icon";
import { StageBadge } from "../components/domain/StageBadge";
import { EmptyState } from "../components/feedback/EmptyState";
import { relativeAge } from "../shell/sync-status";
import type { TaskItemDTO } from "../store/protocol";
import type { TaskState } from "../store/store";
import { Section, SingleColumn } from "./layout";
import { orderDone } from "./done-order";

export interface DoneScreenProps {
  task: TaskState;
  nowMs: number;
}

/** One finished item. Read-only by decision — there is no reopen in the act
 * vocabulary, and none is invented here. */
function DoneRow({ item, nowMs }: { item: TaskItemDTO; nowMs: number }) {
  return (
    <Card
      padding="var(--space-5)"
      style={{ display: "flex", alignItems: "center", gap: "var(--space-5)", flexWrap: "wrap" }}
    >
      <StageBadge stage="done" />
      <span
        style={{
          flex: "1 1 220px",
          minWidth: 0,
          font: "var(--type-body)",
          color: "var(--text-primary)",
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
        }}
      >
        {item.title}
      </span>
      {item.pending ? (
        <span
          title="Not yet confirmed by the server"
          className="hb-meta"
          style={{ display: "inline-flex", alignItems: "center", gap: "var(--space-2)", flex: "0 0 auto" }}
        >
          <Icon name="loader-circle" size={13} />
          Pending
        </span>
      ) : null}
      {/* `updatedAt`, honestly worded: the schema has no completion stamp, so
          "touched" is what this actually is — a later edit re-sorts, and the
          meta never claims "finished N ago". */}
      <span className="hb-meta" style={{ flex: "0 0 auto", whiteSpace: "nowrap" }}>
        touched {relativeAge(Math.max(0, nowMs - item.updatedAt))}
      </span>
    </Card>
  );
}

/** Done: every live item at the Done stage, most recently touched first —
 * the same live-presence rule every other screen uses, so a done item later
 * cancelled drops off here and stays only in the Ledger, labelled.
 * `task.done` is `null` until the first answer arrives: "nothing completed
 * yet" is a claim only a loaded core may make. */
export function DoneScreen({ task, nowMs }: DoneScreenProps) {
  const items = task.done;
  return (
    <SingleColumn>
      <Section
        title="Finished"
        meta={items === null ? "not read yet" : `${items.length} done`}
      >
        {items === null ? (
          <Card padding="var(--space-6)">
            <span className="hb-meta">Waiting for the core's first answer.</span>
          </Card>
        ) : items.length === 0 ? (
          <Card padding="0">
            <EmptyState
              icon="circle-check"
              headingLevel={3}
              title="Nothing completed yet"
              body="Items you complete land here and stay until cancelled."
            />
          </Card>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
            {orderDone(items).map((item) => (
              <DoneRow key={item.id} item={item} nowMs={nowMs} />
            ))}
          </div>
        )}
      </Section>
    </SingleColumn>
  );
}
