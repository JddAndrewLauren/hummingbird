import React from "react";
import { Icon } from "../core/Icon.jsx";

export function Select({ label, options = [], value, onChange, size = "md", id, style = {}, ...rest }) {
  const [focus, setFocus] = React.useState(false);
  const autoId = React.useId ? React.useId() : "hb-select";
  const selectId = id || autoId;
  const h = size === "lg" ? 44 : size === "sm" ? 30 : 36;
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)", ...style }}>
      {label ? <label htmlFor={selectId} style={{ font: "var(--weight-semibold) var(--size-body-sm)/1.2 var(--font-sans)", color: "var(--text-secondary)" }}>{label}</label> : null}
      <div style={{ position: "relative", display: "flex", alignItems: "center" }}>
        <select id={selectId} value={value} onChange={onChange}
          onFocus={() => setFocus(true)} onBlur={() => setFocus(false)}
          style={{ appearance: "none", width: "100%", height: h, padding: "0 var(--space-9) 0 var(--space-5)",
            background: "var(--surface-card)", color: "var(--text-primary)", font: "var(--type-body)",
            border: `1px solid ${focus ? "var(--accent)" : "var(--border-default)"}`,
            borderRadius: "var(--radius-control)", outline: "none",
            boxShadow: focus ? "var(--ring-focus)" : "none", cursor: "pointer",
            transition: "border-color var(--dur-fast) var(--ease-flit)" }} {...rest}>
          {options.map((o) => {
            const opt = typeof o === "string" ? { value: o, label: o } : o;
            return <option key={opt.value} value={opt.value}>{opt.label}</option>;
          })}
        </select>
        <Icon name="chevron-down" size={16} color="var(--text-muted)" style={{ position: "absolute", right: "var(--space-5)", pointerEvents: "none" }} />
      </div>
    </div>
  );
}
