import React from "react";
import { Icon } from "../core/Icon.jsx";

export function Checkbox({ checked = false, onChange, label, hint, tone = "default", disabled = false, style = {}, ...rest }) {
  const [hover, setHover] = React.useState(false);
  const fg = tone === "warn" ? "var(--status-warn-fg)" : "var(--text-primary)";
  return (
    <label
      onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)}
      style={{ display: "flex", alignItems: "flex-start", gap: "var(--space-5)", minHeight: 24,
        cursor: disabled ? "not-allowed" : "pointer", opacity: disabled ? 0.5 : 1, ...style }} {...rest}>
      <input type="checkbox" checked={checked} onChange={onChange} disabled={disabled}
        style={{ position: "absolute", opacity: 0, width: 1, height: 1 }} />
      <span style={{
        display: "inline-flex", alignItems: "center", justifyContent: "center",
        width: 18, height: 18, marginTop: 1, flex: "0 0 auto",
        background: checked ? "var(--accent)" : "var(--surface-card)",
        border: `1px solid ${checked ? "var(--accent)" : hover ? "var(--border-strong)" : "var(--border-default)"}`,
        borderRadius: "var(--radius-xs)", color: "var(--on-accent)",
        transform: checked ? "scale(1)" : "scale(.98)",
        transition: "background var(--dur-fast) var(--ease-flit), border-color var(--dur-fast) var(--ease-flit), transform var(--dur-fast) var(--ease-hover)",
      }}>
        {checked ? <Icon name="check" size={13} strokeWidth={3} /> : null}
      </span>
      <span style={{ display: "flex", flexDirection: "column", gap: 2 }}>
        {label ? <span style={{ font: "var(--type-body)", color: fg }}>{label}</span> : null}
        {hint ? <span style={{ font: "var(--type-body-sm)", color: "var(--text-muted)" }}>{hint}</span> : null}
      </span>
    </label>
  );
}
