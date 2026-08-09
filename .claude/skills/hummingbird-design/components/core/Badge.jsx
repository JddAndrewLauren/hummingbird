import React from "react";
import { Icon } from "./Icon.jsx";

const TONES = {
  neutral: ["var(--text-secondary)", "var(--surface-quiet)", "var(--border-subtle)"],
  brand: ["var(--text-brand)", "var(--accent-quiet)", "var(--accent-quiet-border)"],
  success: ["var(--status-done-fg)", "var(--status-done-bg)", "transparent"],
  warn: ["var(--status-warn-fg)", "var(--status-warn-bg)", "transparent"],
  danger: ["var(--status-danger-fg)", "var(--status-danger-bg)", "transparent"],
  info: ["var(--status-info-fg)", "var(--status-info-bg)", "transparent"],
};

export function Badge({ tone = "neutral", icon, dot = false, mono = false, style = {}, children, ...rest }) {
  const [fg, bg, bd] = TONES[tone] || TONES.neutral;
  return (
    <span style={{
      display: "inline-flex", alignItems: "center", gap: "var(--space-3)",
      height: 22, padding: "0 var(--space-4)", color: fg, background: bg,
      border: `1px solid ${bd}`, borderRadius: "var(--radius-pill)",
      font: mono ? "var(--type-meta)" : "var(--weight-semibold) var(--size-body-sm)/1 var(--font-sans)",
      letterSpacing: mono ? "var(--tracking-meta)" : "0",
      textTransform: mono ? "uppercase" : "none", whiteSpace: "nowrap", ...style,
    }} {...rest}>
      {dot ? <span style={{ width: 6, height: 6, borderRadius: "50%", background: fg }} /> : null}
      {icon ? <Icon name={icon} size={13} /> : null}
      <span style={{ display: "inline-block" }}>{children}</span>
    </span>
  );
}
