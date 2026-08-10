import { Badge } from "../core/Badge";
import { Button } from "../core/Button";
import { Card } from "../core/Card";
import { IconButton } from "../core/IconButton";
import { Checkbox } from "../forms/Checkbox";
import { availableActions } from "../../screens/item-actions";
import { hasPriority, priorityLabel } from "../../screens/priority";
import type { StepDTO, TaskActionName, TaskItemDTO } from "../../store/protocol";
import { StageBadge } from "./StageBadge";

/** One `TaskActionName`'s button label and leading icon — voice per the
 * design README ("Blocked" means an external wait and nothing else, so its
 * label says so rather than leaving the word to speak for itself). */
const ACTION_BUTTON: Record<TaskActionName, { label: string; icon: "play" | "check" | "clock" | "x" }> = {
  start: { label: "Start", icon: "play" },
  complete: { label: "Complete", icon: "check" },
  block: { label: "Mark blocked", icon: "clock" },
  cancel: { label: "Cancel", icon: "x" },
};

export interface ItemDetailPanelProps {
  item: TaskItemDTO;
  /** Whatever `TaskState.stepsByItem[item.id]` currently holds — `[]`
   * until the request answers, same "not yet known" shape every other S9
   * read uses; there is no separate loading flag. */
  steps: StepDTO[];
  onClose: () => void;
  /** S11/#109's act affordances. Omitted (no action row rendered) for a
   * caller that only wants the read-only panel — every existing call site
   * before this slice passed none. */
  onAct?: (action: TaskActionName) => void;
}

/** Item detail: description and Steps (issue #96), read-only from this
 * binding — S11 wires ticking a Step. Priority renders by its label, never
 * the raw wire number (`priorityLabel`), same rule the frontier list's
 * ordering applies.
 *
 * The action row (S11/#109) offers exactly what `availableActions`
 * (`screens/item-actions.ts`) says for `item.stage` — there is no
 * "depends on another action" affordance anywhere here: that is a
 * `blocked_by` relation edge (S10's `getBlocked`), never expressible
 * through this panel's `"block"` button, which only ever sets the funnel's
 * `Blocked` stage (an external wait). Every button disables itself while
 * `item.pending` — a second mutation queued on top of an unconfirmed one
 * would only add a redundant request, never corrupt anything, but there is
 * nothing useful for a person to do with the item until the first one
 * resolves. */
export function ItemDetailPanel({ item, steps, onClose, onAct }: ItemDetailPanelProps) {
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
        {hasPriority(item.priority) ? <Badge tone="brand">{priorityLabel(item.priority)}</Badge> : null}
        {item.size ? <Badge mono>size:{item.size}</Badge> : null}
        {item.energy ? <Badge mono>energy:{item.energy}</Badge> : null}
        {item.context ? <Badge mono>{item.context}</Badge> : null}
      </div>

      {onAct && availableActions(item.stage).length > 0 ? (
        <div style={{ display: "flex", gap: "var(--space-4)", flexWrap: "wrap" }}>
          {availableActions(item.stage).map((action, index) => {
            const { label, icon } = ACTION_BUTTON[action];
            return (
              <Button
                key={action}
                variant={index === 0 ? "primary" : action === "cancel" ? "ghost" : "secondary"}
                iconLeft={icon}
                disabled={item.pending}
                onClick={() => onAct(action)}
              >
                {label}
              </Button>
            );
          })}
        </div>
      ) : null}

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
