import { useState } from "react";
import type React from "react";
import type { ButtonHTMLAttributes, CSSProperties, ReactNode } from "react";
import { Icon, type IconName } from "./Icon";

export interface ButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "style"> {
  /** Renders an `<a href>` wearing this button's skin instead of a
   * `<button>`. For a control whose whole job is to go somewhere: an anchor
   * is announced as a link, appears in a screen reader's link list, and
   * carries middle-click, modifier-click, "Copy link address" and the
   * status-bar preview — none of which a `<button>` calling `window.open`
   * has. Use it whenever the destination is a real URL; keep the button for
   * a control that acts.
   *
   * `disabled` and `loading` are meaningless here — an anchor has no
   * disabled state — and an `href` given with either is treated as the
   * caller's mistake and rendered as an ordinary button instead. */
  href?: string;
  /** Anchor only, and only meaningful beside `href`. `target="_blank"` is
   * paired with `rel="noopener noreferrer"` automatically unless the caller
   * names its own `rel`. */
  target?: string;
  rel?: string;
  /** primary = the one committing action per view. quiet = brand-tinted secondary. */
  variant?: "primary" | "secondary" | "ghost" | "quiet" | "danger";
  /** lg is the touch size (44px); md is desktop default. */
  size?: "sm" | "md" | "lg";
  /** Lucide icon name placed before the label. */
  iconLeft?: IconName;
  /** Lucide icon name placed after the label. */
  iconRight?: IconName;
  loading?: boolean;
  disabled?: boolean;
  fullWidth?: boolean;
  style?: CSSProperties;
  children?: ReactNode;
}

type ButtonVariant = NonNullable<ButtonProps["variant"]>;
type ButtonSize = NonNullable<ButtonProps["size"]>;

interface SizeSpec {
  h: number;
  px: string;
  font: string;
  icon: number;
  gap: string;
}

const SIZES: Record<ButtonSize, SizeSpec> = {
  sm: { h: 30, px: "var(--space-5)", font: "var(--size-body-sm)", icon: 15, gap: "var(--space-3)" },
  md: { h: 36, px: "var(--space-6)", font: "var(--size-body)", icon: 17, gap: "var(--space-4)" },
  lg: { h: 44, px: "var(--space-7)", font: "var(--size-body-lg)", icon: 19, gap: "var(--space-4)" },
};

interface Skin {
  bg: string;
  fg: string;
  bd: string;
  sh: string;
}

function skin(variant: ButtonVariant, state: "hover" | "press" | null): Skin {
  const base = ({
    primary: { bg: "var(--accent)", fg: "var(--on-accent)", bd: "transparent", sh: "var(--shadow-accent)" },
    secondary: { bg: "var(--surface-card)", fg: "var(--text-primary)", bd: "var(--border-default)", sh: "var(--shadow-1)" },
    ghost: { bg: "transparent", fg: "var(--text-secondary)", bd: "transparent", sh: "none" },
    quiet: { bg: "var(--accent-quiet)", fg: "var(--text-brand)", bd: "var(--accent-quiet-border)", sh: "none" },
    danger: { bg: "var(--status-danger-bg)", fg: "var(--status-danger-fg)", bd: "transparent", sh: "none" },
  } satisfies Record<ButtonVariant, Skin>)[variant];
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

export function Button({ variant = "primary", size = "md", iconLeft, iconRight, loading = false, disabled = false, fullWidth = false, type = "button", style = {}, href, target, rel, children, ...rest }: ButtonProps) {
  const [hover, setHover] = useState(false);
  const [press, setPress] = useState(false);
  const s = SIZES[size] || SIZES.md;
  const k = skin(variant, disabled ? null : press ? "press" : hover ? "hover" : null);
  // One skin, two elements. Everything below — the box, the hover lift, the
  // press scale, the icons — is shared; only the tag and the few attributes
  // that belong to it differ.
  const skinStyle: CSSProperties = {
    display: "inline-flex", alignItems: "center", justifyContent: "center", gap: s.gap,
    height: s.h, padding: `0 ${s.px}`, width: fullWidth ? "100%" : undefined,
    font: `var(--weight-semibold) ${s.font}/1 var(--font-sans)`,
    color: k.fg, background: k.bg, border: `1px solid ${k.bd}`,
    borderRadius: "var(--radius-control)", boxShadow: press ? "none" : k.sh,
    cursor: disabled || loading ? "not-allowed" : "pointer",
    opacity: disabled ? 0.45 : 1,
    textDecoration: "none",
    transform: press ? `scale(var(--press-scale))` : hover && !disabled ? `translateY(var(--lift-hover))` : "none",
    transition: "background var(--dur-fast) var(--ease-flit), transform var(--dur-fast) var(--ease-hover), box-shadow var(--dur-fast) var(--ease-flit), border-color var(--dur-fast) var(--ease-flit)",
    whiteSpace: "nowrap", ...style,
  };
  const body = (
    <>
      {loading ? <Icon name="loader-circle" size={s.icon} style={{ animation: "hb-spin 900ms linear infinite" }} /> : iconLeft ? <Icon name={iconLeft} size={s.icon} /> : null}
      <span style={{ display: "inline-block" }}>{children}</span>
      {iconRight ? <Icon name={iconRight} size={s.icon} /> : null}
    </>
  );
  const pointer = {
    onMouseEnter: (event: React.MouseEvent<HTMLElement>) => { setHover(true); rest.onMouseEnter?.(event as never); },
    onMouseLeave: (event: React.MouseEvent<HTMLElement>) => { setHover(false); setPress(false); rest.onMouseLeave?.(event as never); },
    onMouseDown: (event: React.MouseEvent<HTMLElement>) => { setPress(true); rest.onMouseDown?.(event as never); },
    onMouseUp: (event: React.MouseEvent<HTMLElement>) => { setPress(false); rest.onMouseUp?.(event as never); },
  };
  if (href !== undefined && !disabled && !loading) {
    const { type: _type, ...anchorRest } = { type, ...rest };
    void _type; // a `type` belongs to a button, never to an anchor
    return (
      <a
        {...(anchorRest as object)}
        href={href}
        target={target}
        // The pairing every `target="_blank"` in this app already carries;
        // named explicitly by a caller that wants something else.
        rel={rel ?? (target === "_blank" ? "noopener noreferrer" : undefined)}
        {...pointer}
        style={skinStyle}
      >
        {body}
      </a>
    );
  }
  return (
    // `rest` is spread FIRST so the internal pointer handlers below win, and
    // each one calls the caller's handler itself. Spread last (as it was), a
    // caller's `onMouseLeave` would replace the handler that clears
    // hover/press and strand the button in a hovered state.
    <button {...rest} type={type} disabled={disabled || loading} {...pointer} style={skinStyle}>
      {body}
    </button>
  );
}
