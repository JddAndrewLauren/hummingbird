import React from "react";
import { Icon } from "../core/Icon.jsx";

export function Input({ label, hint, error, icon, size = "md", trailing, id, style = {}, ...rest }) {
  const [focus, setFocus] = React.useState(false);
  const autoId = React.useId ? React.useId() : "hb-input";
  const inputId = id || autoId;
  // One hint/error node, one id, derived from the same base as the field — so a
  // caller-supplied `id` owns both ends of the association. Only point
  // aria-describedby at it when something is actually rendered there.
  const describedById = inputId + "-desc";
  const describedBy = error || hint ? describedById : undefined;
  const h = size === "lg" ? 44 : size === "sm" ? 30 : 36;
  const borderColor = error ? "var(--status-danger-fg)" : focus ? "var(--accent)" : "var(--border-default)";
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)", ...style }}>
      {label ? <label htmlFor={inputId} style={{ font: "var(--weight-semibold) var(--size-body-sm)/1.2 var(--font-sans)", color: "var(--text-secondary)" }}>{label}</label> : null}
      <div style={{
        display: "flex", alignItems: "center", gap: "var(--space-4)", height: h,
        padding: "0 var(--space-5)", background: "var(--surface-card)",
        border: `1px solid ${borderColor}`, borderRadius: "var(--radius-control)",
        boxShadow: focus ? "var(--ring-focus)" : "none",
        transition: "border-color var(--dur-fast) var(--ease-flit), box-shadow var(--dur-fast) var(--ease-flit)",
      }}>
        {icon ? <Icon name={icon} size={16} color="var(--text-muted)" /> : null}
        <input id={inputId} aria-describedby={describedBy} aria-invalid={error ? true : undefined} {...rest}
          onFocus={(event) => { setFocus(true); rest.onFocus?.(event); }}
          onBlur={(event) => { setFocus(false); rest.onBlur?.(event); }}
          style={{ flex: 1, minWidth: 0, border: "none", outline: "none", background: "transparent",
            font: "var(--type-body)", color: "var(--text-primary)", padding: 0 }} />
        {trailing}
      </div>
      {error ? <span id={describedById} style={{ font: "var(--type-body-sm)", color: "var(--status-danger-fg)" }}>{error}</span>
        : hint ? <span id={describedById} style={{ font: "var(--type-body-sm)", color: "var(--text-muted)" }}>{hint}</span> : null}
    </div>
  );
}
