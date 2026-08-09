import { useState } from "react";
import type { ChangeEventHandler, CSSProperties, LabelHTMLAttributes, ReactNode } from "react";

export interface SwitchProps extends Omit<LabelHTMLAttributes<HTMLLabelElement>, "onChange" | "style"> {
  checked?: boolean;
  onChange?: ChangeEventHandler<HTMLInputElement>;
  label?: ReactNode;
  hint?: ReactNode;
  disabled?: boolean;
  style?: CSSProperties;
}

/** Ask the real input whether it matches :focus-visible, so a pointer click on the
    label does not light the ring. Engines without the selector throw on it; those
    fall back to ringing on any focus, which beats no ring at all. */
function keyboardFocused(el: HTMLElement) {
  try { return el.matches(":focus-visible"); } catch { return true; }
}

export function Switch({ checked = false, onChange, label, hint, disabled = false, style = {}, ...rest }: SwitchProps) {
  const [focus, setFocus] = useState(false);
  return (
    <label {...rest}
      style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "var(--space-6)",
        minHeight: 32, cursor: disabled ? "not-allowed" : "pointer", opacity: disabled ? 0.5 : 1, ...style }}>
      <span style={{ display: "flex", flexDirection: "column", gap: 2 }}>
        {label ? <span style={{ font: "var(--type-body)", color: "var(--text-primary)" }}>{label}</span> : null}
        {hint ? <span style={{ font: "var(--type-body-sm)", color: "var(--text-muted)" }}>{hint}</span> : null}
      </span>
      {/* readOnly when there is no handler: a controlled input without onChange
          warns in React and announces as togglable when it is not. */}
      <input type="checkbox" role="switch" checked={checked} onChange={onChange} readOnly={!onChange} disabled={disabled}
        onFocus={(e) => setFocus(keyboardFocused(e.currentTarget))} onBlur={() => setFocus(false)}
        style={{ position: "absolute", opacity: 0, width: 1, height: 1 }} />
      <span style={{ position: "relative", width: 40, height: 24, flex: "0 0 auto",
        background: checked ? "var(--accent)" : "var(--border-strong)",
        borderRadius: "var(--radius-pill)",
        boxShadow: focus ? "var(--ring-focus)" : "none",
        transition: "background var(--dur-base) var(--ease-flit), box-shadow var(--dur-fast) var(--ease-flit)" }}>
        <span style={{ position: "absolute", top: 3, left: checked ? 19 : 3, width: 18, height: 18,
          background: "var(--on-accent)", borderRadius: "50%", boxShadow: "var(--shadow-1)",
          transition: "left var(--dur-base) var(--ease-hover)" }} />
      </span>
    </label>
  );
}
