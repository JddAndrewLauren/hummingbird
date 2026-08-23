import { useId, useState } from "react";
import type { CSSProperties, ReactNode, SelectHTMLAttributes } from "react";
import { Icon } from "../core/Icon";

export interface SelectOption {
  value: string;
  label: string;
  /** Rendered but unpickable. For a vocabulary whose members can be
   * *retired* rather than removed — the rules screen's source list, whose
   * retired entries must still name themselves on an existing rule while
   * refusing a fresh pick the authority would 400 anyway. */
  disabled?: boolean;
}

export interface SelectProps extends Omit<SelectHTMLAttributes<HTMLSelectElement>, "size" | "style"> {
  /** A node rather than a string, so a caller can set a glyph beside the
   * field name — a native `<option>` cannot carry an SVG, so a per-level
   * icon has nowhere else to go (#446, `screens/TriageRow.tsx`). Plain
   * strings still work and are still the common case. */
  label?: ReactNode;
  /** Strings are used as both value and label. */
  options?: Array<string | SelectOption>;
  size?: "sm" | "md" | "lg";
  style?: CSSProperties;
}

export function Select({ label, options = [], value, onChange, size = "md", id, style = {}, ...rest }: SelectProps) {
  const [focus, setFocus] = useState(false);
  const autoId = useId();
  const selectId = id || autoId;
  const h = size === "lg" ? 44 : size === "sm" ? 30 : 36;
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)", ...style }}>
      {label ? <label htmlFor={selectId} style={{ display: "inline-flex", alignItems: "center", gap: "var(--space-2)", font: "var(--weight-semibold) var(--size-body-sm)/1.2 var(--font-sans)", color: "var(--text-secondary)" }}>{label}</label> : null}
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
            const opt: SelectOption = typeof o === "string" ? { value: o, label: o } : o;
            return <option key={opt.value} value={opt.value} disabled={opt.disabled}>{opt.label}</option>;
          })}
        </select>
        <Icon name="chevron-down" size={16} color="var(--text-muted)" style={{ position: "absolute", right: "var(--space-5)", pointerEvents: "none" }} />
      </div>
    </div>
  );
}
