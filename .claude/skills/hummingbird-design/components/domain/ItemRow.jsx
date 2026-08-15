import React from "react";
import { Icon } from "../core/Icon.jsx";
import { StageBadge } from "./StageBadge.jsx";

const URGENCY = { calm: "var(--urgency-calm)", soon: "var(--urgency-soon)", now: "var(--urgency-now)", overdue: "var(--urgency-overdue)" };
// The dot carries urgency in colour alone, so its tooltip says it in words —
// the stored enum is not what a reader wants hovering a coloured dot.
const URGENCY_LABEL = { calm: "Calm", soon: "Deadline soon", now: "Deadline now", overdue: "Overdue" };

export function ItemRow({ title, stage = "ready", urgency = "calm", deadline, scheduled, size, blockedBy, steps, selected = false, onClick, onKeyDown, onMouseEnter, onMouseLeave, style = {}, ...rest }) {
  const [hover, setHover] = React.useState(false);
  // No onClick, no affordance: a row that does nothing must not take focus,
  // announce itself as a button, or claim a pointer.
  const activatable = Boolean(onClick);
  return (
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
