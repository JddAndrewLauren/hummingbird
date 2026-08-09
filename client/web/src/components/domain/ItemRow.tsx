import { useState } from "react";
import type { CSSProperties, HTMLAttributes } from "react";
import { Icon } from "../core/Icon";
import { StageBadge } from "./StageBadge";
import type { Stage } from "./StageBadge";

export interface ItemRowProps extends Omit<HTMLAttributes<HTMLDivElement>, "style"> {
  title: string;
  stage?: Stage;
  /** Derived at read time, never stored — see CONTEXT.md "Urgency". */
  urgency?: "calm" | "soon" | "now" | "overdue";
  /** Deadline the world imposes. Rendered with a flag. */
  due?: string;
  /** Do-date the human chose. Rendered with a calendar glyph, always muted. */
  scheduled?: string;
  /** Size label: quick · normal · deep. */
  size?: string;
  /** Count or key of the actions this one is blocked by. */
  blockedBy?: string;
  /** Microtask progress, e.g. "2/5". */
  steps?: string;
  selected?: boolean;
  style?: CSSProperties;
}

const URGENCY: Record<"calm" | "soon" | "now" | "overdue", string> = { calm: "var(--urgency-calm)", soon: "var(--urgency-soon)", now: "var(--urgency-now)", overdue: "var(--urgency-overdue)" };

export function ItemRow({ title, stage = "ready", urgency = "calm", due, scheduled, size, blockedBy, steps, selected = false, onClick, style = {}, ...rest }: ItemRowProps) {
  const [hover, setHover] = useState(false);
  return (
    <div role="button" tabIndex={0} onClick={onClick}
      onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)}
      style={{
        display: "flex", alignItems: "center", gap: "var(--space-5)",
        minHeight: "var(--row-height)", padding: "var(--space-4) var(--space-5)",
        background: selected ? "var(--accent-quiet)" : hover ? "var(--surface-quiet)" : "transparent",
        borderLeft: `2px solid ${selected ? "var(--accent)" : "transparent"}`,
        borderRadius: "var(--radius-sm)", cursor: "pointer",
        transition: "background var(--dur-fast) var(--ease-flit)", ...style,
      }} {...rest}>
      <span title={urgency} style={{ width: 6, height: 6, borderRadius: "50%", flex: "0 0 auto", background: URGENCY[urgency] || URGENCY.calm }} />
      <span style={{ flex: 1, minWidth: 0, font: "var(--type-body)", color: stage === "done" ? "var(--text-muted)" : "var(--text-primary)",
        textDecoration: stage === "done" ? "line-through" : "none", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{title}</span>
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
      {scheduled ? (
        <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--space-2)", font: "var(--type-meta)", color: "var(--text-muted)" }}>
          <Icon name="calendar" size={13} />{scheduled}
        </span>
      ) : null}
      {due ? (
        <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--space-2)", font: "var(--type-meta)",
          color: urgency === "overdue" ? "var(--urgency-overdue)" : urgency === "now" ? "var(--urgency-now)" : "var(--text-secondary)" }}>
          <Icon name="flag" size={13} />{due}
        </span>
      ) : null}
      <StageBadge stage={stage} />
    </div>
  );
}
