import { useState } from "react";
import type { CSSProperties, HTMLAttributes } from "react";
import { Icon } from "../core/Icon";
import { hasPriority, priorityLabel } from "../../screens/priority";
import { StageBadge } from "./StageBadge";
import type { Stage } from "./StageBadge";

export interface ItemRowProps extends Omit<HTMLAttributes<HTMLDivElement>, "style"> {
  title: string;
  stage?: Stage;
  /** Derived at read time, never stored — see CONTEXT.md "Urgency". */
  urgency?: "calm" | "soon" | "now" | "overdue";
  /** Deadline the world imposes. Rendered with a flag. */
  deadline?: string;
  /** Do-date the human chose. Rendered with a calendar glyph, always muted. */
  scheduled?: string;
  /** Size label: quick · normal · deep. */
  size?: string;
  /** The owned schema's raw `items.priority` wire value (0..4, ADR-0009) —
   * rendered by its label (`priorityLabel`), never the raw number, which is
   * inverted and holed (issue #108). Omitted entirely at "No priority"
   * (0), the same "nothing to say" contract every other optional meta chip
   * on this row already follows. */
  priority?: number;
  /** Count or key of the actions this one is blocked by. */
  blockedBy?: string;
  /** Microtask progress, e.g. "2/5". */
  steps?: string;
  /** Set once an unconfirmed capture/mutation is overlaid on this item
   * (`TaskState.pending`, `Core::is_pending`) — a pending item must be
   * marked as such (issue #108) rather than rendered indistinguishably
   * from confirmed server truth. */
  pending?: boolean;
  selected?: boolean;
  style?: CSSProperties;
}

const URGENCY: Record<"calm" | "soon" | "now" | "overdue", string> = { calm: "var(--urgency-calm)", soon: "var(--urgency-soon)", now: "var(--urgency-now)", overdue: "var(--urgency-overdue)" };
// The dot carries urgency in colour alone, so its tooltip says it in words —
// the stored enum is not what a reader wants hovering a coloured dot.
const URGENCY_LABEL: Record<"calm" | "soon" | "now" | "overdue", string> = { calm: "Calm", soon: "Due soon", now: "Due now", overdue: "Overdue" };

export function ItemRow({ title, stage = "ready", urgency = "calm", deadline, scheduled, size, priority, blockedBy, steps, pending = false, selected = false, onClick, onKeyDown, onMouseEnter, onMouseLeave, style = {}, ...rest }: ItemRowProps) {
  const [hover, setHover] = useState(false);
  // No onClick, no affordance: a row that does nothing must not take focus,
  // announce itself as a button, or claim a pointer.
  const activatable = Boolean(onClick);
  return (
    // The role, the tab stop and Enter/Space activation all arrive together
    // with `onClick`; without it the row is inert text with hover paint.
    // (jsx-a11y reads this ternary role only because the config enables
    // `allowExpressionValues` for exactly this case.)
    <div role={activatable ? "button" : undefined} tabIndex={activatable ? 0 : undefined} onClick={onClick}
      onKeyDown={(event) => {
        // onClick on a div never fires from the keyboard, so Enter and Space
        // are wired by hand; Space is prevented first or it scrolls the list.
        if (activatable && (event.key === "Enter" || event.key === " ")) {
          event.preventDefault();
          event.currentTarget.click();
        }
        onKeyDown?.(event);
      }}
      onMouseEnter={(event) => { setHover(true); onMouseEnter?.(event); }}
      onMouseLeave={(event) => { setHover(false); onMouseLeave?.(event); }}
      style={{
        display: "flex", alignItems: "center", gap: "var(--space-5)",
        minHeight: "var(--row-height)", padding: "var(--space-4) var(--space-5)",
        background: selected ? "var(--accent-quiet)" : hover ? "var(--surface-quiet)" : "transparent",
        borderLeft: `2px solid ${selected ? "var(--accent)" : "transparent"}`,
        borderRadius: "var(--radius-sm)", cursor: activatable ? "pointer" : undefined,
        transition: "background var(--dur-fast) var(--ease-flit)", ...style,
      }} {...rest}>
      <span title={URGENCY_LABEL[urgency] || URGENCY_LABEL.calm} style={{ width: 6, height: 6, borderRadius: "50%", flex: "0 0 auto", background: URGENCY[urgency] || URGENCY.calm }} />
      {/* No `opacity` change here for `pending`: the chip below is the one
          pending indicator, deliberately, so a caller that also dims this
          row for an unrelated reason (e.g. NowScreen's "Blocked" section)
          never compounds two opacities into an over-muted row
          (PR #200 review). */}
      <span style={{ flex: 1, minWidth: 0, font: "var(--type-body)", color: stage === "done" ? "var(--text-muted)" : "var(--text-primary)",
        textDecoration: stage === "done" ? "line-through" : "none", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{title}</span>
      {pending ? (
        <span title="Not yet confirmed by the server" style={{ display: "inline-flex", alignItems: "center", gap: "var(--space-2)", font: "var(--type-meta)", letterSpacing: "var(--tracking-meta)", textTransform: "uppercase", color: "var(--text-muted)" }}>
          <Icon name="loader-circle" size={13} />Pending
        </span>
      ) : null}
      {steps ? (
        <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--space-2)", font: "var(--type-meta)", color: "var(--text-muted)" }}>
          <Icon name="list-checks" size={13} />{steps}
        </span>
      ) : null}
      {blockedBy ? (
        <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--space-2)", font: "var(--type-meta)", color: "var(--status-danger-fg)" }}>
          <Icon name="link" size={13} />{blockedBy}
        </span>
      ) : null}
      {size ? <span style={{ font: "var(--type-meta)", letterSpacing: "var(--tracking-meta)", textTransform: "uppercase", color: "var(--text-muted)" }}>{size}</span> : null}
      {priority !== undefined && hasPriority(priority) ? (
        <span style={{ font: "var(--type-meta)", letterSpacing: "var(--tracking-meta)", textTransform: "uppercase", color: "var(--text-brand)" }}>{priorityLabel(priority)}</span>
      ) : null}
      {scheduled ? (
        <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--space-2)", font: "var(--type-meta)", color: "var(--text-muted)" }}>
          <Icon name="calendar" size={13} />{scheduled}
        </span>
      ) : null}
      {deadline ? (
        <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--space-2)", font: "var(--type-meta)",
          color: urgency === "overdue" ? "var(--urgency-overdue)" : urgency === "now" ? "var(--urgency-now)" : "var(--text-secondary)" }}>
          <Icon name="flag" size={13} />{deadline}
        </span>
      ) : null}
      <StageBadge stage={stage} />
    </div>
  );
}
