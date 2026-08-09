import React from "react";

export function Card({ elevation = 1, padding = "var(--space-6)", interactive = false, accent = false, as: Tag = "div", style = {}, children, ...rest }) {
  const [hover, setHover] = React.useState(false);
  const shadow = ["var(--shadow-0)", "var(--shadow-1)", "var(--shadow-2)", "var(--shadow-3)"][elevation] || "var(--shadow-1)";
  return (
    <Tag
      onMouseEnter={interactive ? () => setHover(true) : undefined}
      onMouseLeave={interactive ? () => setHover(false) : undefined}
      style={{
        background: "var(--surface-card)", padding,
        border: `1px solid ${accent ? "var(--accent-quiet-border)" : "var(--border-subtle)"}`,
        borderRadius: "var(--radius-card)",
        boxShadow: interactive && hover ? "var(--shadow-2)" : shadow,
        transform: interactive && hover ? "translateY(var(--lift-hover))" : "none",
        transition: "box-shadow var(--dur-base) var(--ease-flit), transform var(--dur-base) var(--ease-hover), border-color var(--dur-fast) var(--ease-flit)",
        cursor: interactive ? "pointer" : undefined,
        ...style,
      }} {...rest}>
      {children}
    </Tag>
  );
}
