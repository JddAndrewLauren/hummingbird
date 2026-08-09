import React from "react";
import { Icon } from "./Icon.jsx";

const BOX = { sm: 28, md: 34, lg: 44 };

export function IconButton({ icon, label, size = "md", variant = "ghost", active = false, disabled = false, style = {}, ...rest }) {
  const [hover, setHover] = React.useState(false);
  const [press, setPress] = React.useState(false);
  const box = BOX[size] || BOX.md;
  const bg = active ? "var(--accent-quiet)" : hover && !disabled ? "var(--surface-quiet)" : variant === "solid" ? "var(--surface-card)" : "transparent";
  return (
    // `rest` first so the pointer handlers below win and call the caller's
    // themselves; spread last, a caller's `onMouseLeave` would replace the
    // one that clears hover/press and leave the control stuck hovered.
    <button {...rest} type="button" aria-label={label} title={label} disabled={disabled}
      onMouseEnter={(e) => { setHover(true); rest.onMouseEnter?.(e); }}
      onMouseLeave={(e) => { setHover(false); setPress(false); rest.onMouseLeave?.(e); }}
      onMouseDown={(e) => { setPress(true); rest.onMouseDown?.(e); }}
      onMouseUp={(e) => { setPress(false); rest.onMouseUp?.(e); }}
      style={{
        display: "inline-flex", alignItems: "center", justifyContent: "center",
        width: box, height: box, padding: 0, background: bg,
        color: active ? "var(--text-brand)" : "var(--text-secondary)",
        border: variant === "solid" ? "1px solid var(--border-default)" : "1px solid transparent",
        borderRadius: "var(--radius-control)", cursor: disabled ? "not-allowed" : "pointer",
        opacity: disabled ? 0.45 : 1,
        transform: press ? "scale(var(--press-scale))" : "none",
        transition: "background var(--dur-fast) var(--ease-flit), color var(--dur-fast) var(--ease-flit), transform var(--dur-fast) var(--ease-hover)",
        ...style,
      }}>
      <Icon name={icon} size={size === "lg" ? 20 : size === "sm" ? 15 : 18} />
    </button>
  );
}
