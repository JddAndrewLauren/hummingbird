import { useState } from "react";
import type { ChangeEventHandler, CSSProperties, LabelHTMLAttributes, ReactNode } from "react";
import { Icon } from "../core/Icon";

export interface CheckboxProps extends Omit<LabelHTMLAttributes<HTMLLabelElement>, "onChange" | "style"> {
  checked?: boolean;
  onChange?: ChangeEventHandler<HTMLInputElement>;
  label?: ReactNode;
  /** Second line under the label, muted. */
  hint?: ReactNode;
  /** "warn" turns the label amber — used for an unavailable-but-selected calendar. */
  tone?: "default" | "warn";
  disabled?: boolean;
  style?: CSSProperties;
}

/** Ask the real input whether it matches :focus-visible, so a pointer click on the
    label does not light the ring. Engines without the selector throw on it; those
    fall back to ringing on any focus, which beats no ring at all. */
function keyboardFocused(el: HTMLElement) {
  try { return el.matches(":focus-visible"); } catch { return true; }
}

export function Checkbox({ checked = false, onChange, label, hint, tone = "default", disabled = false, style = {}, ...rest }: CheckboxProps) {
  const [hover, setHover] = useState(false);
  const [focus, setFocus] = useState(false);
  const fg = tone === "warn" ? "var(--status-warn-fg)" : "var(--text-primary)";
  return (
    <label
      {...rest}
      onMouseEnter={(e) => { setHover(true); rest.onMouseEnter?.(e); }}
      onMouseLeave={(e) => { setHover(false); rest.onMouseLeave?.(e); }}
      style={{ display: "flex", alignItems: "flex-start", gap: "var(--space-5)", minHeight: 24,
        cursor: disabled ? "not-allowed" : "pointer", opacity: disabled ? 0.5 : 1, ...style }}>
      {/* readOnly when there is no handler: a controlled input without onChange
          warns in React and announces as togglable when it is not. */}
      <input type="checkbox" checked={checked} onChange={onChange} readOnly={!onChange} disabled={disabled}
        onFocus={(e) => setFocus(keyboardFocused(e.currentTarget))} onBlur={() => setFocus(false)}
        style={{ position: "absolute", opacity: 0, width: 1, height: 1 }} />
      <span style={{
        display: "inline-flex", alignItems: "center", justifyContent: "center",
        width: 18, height: 18, marginTop: 1, flex: "0 0 auto",
        background: checked ? "var(--accent)" : "var(--surface-card)",
        border: `1px solid ${checked ? "var(--accent)" : hover ? "var(--border-strong)" : "var(--border-default)"}`,
        borderRadius: "var(--radius-xs)", color: "var(--on-accent)",
        boxShadow: focus ? "var(--ring-focus)" : "none",
        transform: checked ? "scale(1)" : "scale(.98)",
        transition: "background var(--dur-fast) var(--ease-flit), border-color var(--dur-fast) var(--ease-flit), box-shadow var(--dur-fast) var(--ease-flit), transform var(--dur-fast) var(--ease-hover)",
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
