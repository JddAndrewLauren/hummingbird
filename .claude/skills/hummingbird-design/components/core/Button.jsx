import React from "react";
import { Icon } from "./Icon.jsx";

const SIZES = {
  sm: { h: 30, px: "var(--space-5)", font: "var(--size-body-sm)", icon: 15, gap: "var(--space-3)" },
  md: { h: 36, px: "var(--space-6)", font: "var(--size-body)", icon: 17, gap: "var(--space-4)" },
  lg: { h: 44, px: "var(--space-7)", font: "var(--size-body-lg)", icon: 19, gap: "var(--space-4)" },
};

function skin(variant, state) {
  const base = {
    primary: { bg: "var(--accent)", fg: "var(--on-accent)", bd: "transparent", sh: "var(--shadow-accent)" },
    secondary: { bg: "var(--surface-card)", fg: "var(--text-primary)", bd: "var(--border-default)", sh: "var(--shadow-1)" },
    ghost: { bg: "transparent", fg: "var(--text-secondary)", bd: "transparent", sh: "none" },
    quiet: { bg: "var(--accent-quiet)", fg: "var(--text-brand)", bd: "var(--accent-quiet-border)", sh: "none" },
    danger: { bg: "var(--status-danger-bg)", fg: "var(--status-danger-fg)", bd: "transparent", sh: "none" },
  }[variant] || {};
  if (state === "hover") {
    if (variant === "primary") return { ...base, bg: "var(--accent-hover)" };
    if (variant === "ghost") return { ...base, bg: "var(--surface-quiet)", fg: "var(--text-primary)" };
    if (variant === "secondary") return { ...base, bd: "var(--border-strong)", sh: "var(--shadow-2)" };
    if (variant === "quiet") return { ...base, bg: "color-mix(in oklab, var(--accent-quiet) 70%, var(--accent) 12%)" };
    if (variant === "danger") return { ...base, bg: "color-mix(in oklab, var(--status-danger-bg) 70%, var(--status-danger-fg) 14%)" };
  }
  if (state === "press" && variant === "primary") return { ...base, bg: "var(--accent-press)" };
  return base;
}

export function Button({ variant = "primary", size = "md", iconLeft, iconRight, loading = false, disabled = false, fullWidth = false, type = "button", style = {}, children, ...rest }) {
  const [hover, setHover] = React.useState(false);
  const [press, setPress] = React.useState(false);
  const s = SIZES[size] || SIZES.md;
  const k = skin(variant, disabled ? null : press ? "press" : hover ? "hover" : null);
  return (
    // `rest` is spread FIRST so the pointer handlers below win, and each one
    // calls the caller's handler itself. Spread last, a caller's
    // `onMouseLeave` would replace the handler that clears hover/press and
    // strand the button in a hovered state.
    <button {...rest} type={type} disabled={disabled || loading}
      onMouseEnter={(e) => { setHover(true); rest.onMouseEnter?.(e); }}
      onMouseLeave={(e) => { setHover(false); setPress(false); rest.onMouseLeave?.(e); }}
      onMouseDown={(e) => { setPress(true); rest.onMouseDown?.(e); }}
      onMouseUp={(e) => { setPress(false); rest.onMouseUp?.(e); }}
      style={{
        display: "inline-flex", alignItems: "center", justifyContent: "center", gap: s.gap,
        height: s.h, padding: `0 ${s.px}`, width: fullWidth ? "100%" : undefined,
        font: `var(--weight-semibold) ${s.font}/1 var(--font-sans)`,
        color: k.fg, background: k.bg, border: `1px solid ${k.bd}`,
        borderRadius: "var(--radius-control)", boxShadow: press ? "none" : k.sh,
        cursor: disabled || loading ? "not-allowed" : "pointer",
        opacity: disabled ? 0.45 : 1,
        transform: press ? `scale(var(--press-scale))` : hover && !disabled ? `translateY(var(--lift-hover))` : "none",
        transition: "background var(--dur-fast) var(--ease-flit), transform var(--dur-fast) var(--ease-hover), box-shadow var(--dur-fast) var(--ease-flit), border-color var(--dur-fast) var(--ease-flit)",
        whiteSpace: "nowrap", ...style,
      }}>
      {loading ? <Icon name="loader-circle" size={s.icon} style={{ animation: "hb-spin 900ms linear infinite" }} /> : iconLeft ? <Icon name={iconLeft} size={s.icon} /> : null}
      <span style={{ display: "inline-block" }}>{children}</span>
      {iconRight ? <Icon name={iconRight} size={s.icon} /> : null}
    </button>
  );
}
