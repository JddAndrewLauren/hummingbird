import React from "react";
import { Icon } from "../core/Icon.jsx";

export function EmptyState({ icon = "feather", title, body, action, compact = false, headingLevel = 2, style = {}, ...rest }) {
  // The title reads as a heading, so it is one: `--type-h3` is the size token,
  // not the level, and the level belongs to wherever the state is placed.
  const Heading = `h${headingLevel}`;
  return (
    <div style={{
      display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center",
      gap: "var(--space-4)", padding: compact ? "var(--space-8)" : "var(--space-11) var(--space-8)",
      textAlign: "center", ...style,
    }} {...rest}>
      <span style={{ display: "inline-flex", alignItems: "center", justifyContent: "center",
        width: 44, height: 44, borderRadius: "var(--radius-pill)",
        background: "var(--accent-quiet)", color: "var(--text-brand)" }}>
        <Icon name={icon} size={20} />
      </span>
      <Heading style={{ font: "var(--type-h3)", color: "var(--text-primary)", margin: 0 }}>{title}</Heading>
      {body ? <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)", maxWidth: 320 }}>{body}</p> : null}
      {action ? <div style={{ marginTop: "var(--space-3)" }}>{action}</div> : null}
    </div>
  );
}
