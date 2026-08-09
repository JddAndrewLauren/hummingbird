import type { ChangeEventHandler, CSSProperties, ReactNode } from "react";

export interface SwitchProps {
  checked?: boolean;
  onChange?: ChangeEventHandler<HTMLInputElement>;
  label?: ReactNode;
  hint?: ReactNode;
  disabled?: boolean;
  style?: CSSProperties;
}

export function Switch({ checked = false, onChange, label, hint, disabled = false, style = {}, ...rest }: SwitchProps) {
  return (
    <label style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "var(--space-6)",
      minHeight: 32, cursor: disabled ? "not-allowed" : "pointer", opacity: disabled ? 0.5 : 1, ...style }} {...rest}>
      <span style={{ display: "flex", flexDirection: "column", gap: 2 }}>
        {label ? <span style={{ font: "var(--type-body)", color: "var(--text-primary)" }}>{label}</span> : null}
        {hint ? <span style={{ font: "var(--type-body-sm)", color: "var(--text-muted)" }}>{hint}</span> : null}
      </span>
      {/* readOnly when there is no handler: a controlled input without onChange
          warns in React and announces as togglable when it is not. */}
      <input type="checkbox" role="switch" checked={checked} onChange={onChange} readOnly={!onChange} disabled={disabled}
        style={{ position: "absolute", opacity: 0, width: 1, height: 1 }} />
      <span style={{ position: "relative", width: 40, height: 24, flex: "0 0 auto",
        background: checked ? "var(--accent)" : "var(--border-strong)",
        borderRadius: "var(--radius-pill)",
        transition: "background var(--dur-base) var(--ease-flit)" }}>
        <span style={{ position: "absolute", top: 3, left: checked ? 19 : 3, width: 18, height: 18,
          background: "#fff", borderRadius: "50%", boxShadow: "var(--shadow-1)",
          transition: "left var(--dur-base) var(--ease-hover)" }} />
      </span>
    </label>
  );
}
