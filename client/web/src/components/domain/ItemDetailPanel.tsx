import { Badge } from "../core/Badge";
import { Card } from "../core/Card";
import { IconButton } from "../core/IconButton";
import { Checkbox } from "../forms/Checkbox";
import { priorityLabel } from "../../screens/priority";
import type { StepDTO, TaskItemDTO } from "../../store/protocol";
import { StageBadge } from "./StageBadge";

export interface ItemDetailPanelProps {
  item: TaskItemDTO;
  /** Whatever `TaskState.stepsByItem[item.id]` currently holds — `[]`
   * until the request answers, same "not yet known" shape every other S9
   * read uses; there is no separate loading flag. */
  steps: StepDTO[];
  onClose: () => void;
}

/** Item detail: description and Steps (issue #96), read-only from this
 * binding — S11 wires ticking a Step. Priority renders by its label, never
 * the raw wire number (`priorityLabel`), same rule the frontier list's
 * ordering applies. */
export function ItemDetailPanel({ item, steps, onClose }: ItemDetailPanelProps) {
  return (
    <Card
      elevation={2}
      padding="var(--space-5)"
      style={{ display: "flex", flexDirection: "column", gap: "var(--space-5)" }}
    >
      <div style={{ display: "flex", alignItems: "flex-start", justifyContent: "space-between" }}>
        <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
          <span className="hb-meta">{item.seq !== null ? `HB-${item.seq}` : "item detail"}</span>
          <h2 style={{ font: "var(--type-h3)", color: "var(--text-primary)" }}>{item.title}</h2>
        </div>
        <IconButton icon="x" label="Close item detail" onClick={onClose} />
      </div>

      <div style={{ display: "flex", gap: "var(--space-3)", flexWrap: "wrap" }}>
        <StageBadge stage={item.stage} />
        {item.priority !== 0 ? <Badge tone="brand">{priorityLabel(item.priority)}</Badge> : null}
        {item.size ? <Badge mono>size:{item.size}</Badge> : null}
        {item.energy ? <Badge mono>energy:{item.energy}</Badge> : null}
        {item.context ? <Badge mono>{item.context}</Badge> : null}
      </div>

      {item.description ? (
        <p style={{ font: "var(--type-body)", color: "var(--text-secondary)" }}>{item.description}</p>
      ) : null}

      <div>
        <span className="hb-meta">steps</span>
        {steps.length === 0 ? (
          <p
            style={{
              font: "var(--type-body-sm)",
              color: "var(--text-muted)",
              marginTop: "var(--space-3)",
            }}
          >
            No Steps yet.
          </p>
        ) : (
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              gap: "var(--space-4)",
              marginTop: "var(--space-3)",
            }}
          >
            {steps.map((step) => (
              <Checkbox key={step.id} checked={step.done} label={step.body} />
            ))}
          </div>
        )}
      </div>
    </Card>
  );
}
