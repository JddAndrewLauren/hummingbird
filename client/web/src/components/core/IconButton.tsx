import { useState } from "react";
import type { ButtonHTMLAttributes, CSSProperties } from "react";
import { Icon, type IconName } from "./Icon";

export interface IconButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "style"> {
  /** Lucide icon name. */
  icon: IconName;
  /** Required — becomes both aria-label and the tooltip. */
  label: string;
  size?: "sm" | "md" | "lg";
  variant?: "ghost" | "solid";
  /** Toggled-on state: brand tint + brand foreground. */
  active?: boolean;
  /**
   * A solid coloured square with a white glyph, for a control that says what
   * it is by colour alone. Overrides `variant`/`active`'s background and the
   * secondary foreground; `neutral` (the default) leaves both untouched.
   *
   * `info` pins `--sky-600` rather than `--stage-triage`, whose dark-mode
   * value is the light `#5cb6d8` and cannot carry white content — one blue
   * across both themes, the way Compose keeps `primary = Ember500`.
   */
  tone?: "neutral" | "info" | "accent";
  disabled?: boolean;
  style?: CSSProperties;
}

type IconButtonSize = NonNullable<IconButtonProps["size"]>;

const BOX: Record<IconButtonSize, number> = { sm: 28, md: 34, lg: 44 };

/** Rest / hover / press fill for each non-neutral tone. */
const TONE_FILL: Record<"info" | "accent", [string, string, string]> = {
  accent: ["var(--accent)", "var(--accent-hover)", "var(--accent-press)"],
  info: ["var(--sky-600)", "var(--sky-900)", "var(--sky-900)"],
};

export function IconButton({ icon, label, size = "md", variant = "ghost", active = false, tone = "neutral", disabled = false, style = {}, ...rest }: IconButtonProps) {
  const [hover, setHover] = useState(false);
  const [press, setPress] = useState(false);
  const box = BOX[size] || BOX.md;
  const fill = tone === "neutral" ? null : TONE_FILL[tone];
  const bg = fill
    ? (press && !disabled ? fill[2] : hover && !disabled ? fill[1] : fill[0])
    : active ? "var(--accent-quiet)" : hover && !disabled ? "var(--surface-quiet)" : variant === "solid" ? "var(--surface-card)" : "transparent";
  return (
    // `rest` first so the internal pointer handlers win and call the caller's
    // themselves; spread last, a caller's `onMouseLeave` would replace the
    // one that clears hover/press and leave the control stuck hovered.
    <button {...rest} type="button" aria-label={label} title={label} disabled={disabled}
      onMouseEnter={(event) => { setHover(true); rest.onMouseEnter?.(event); }}
      onMouseLeave={(event) => { setHover(false); setPress(false); rest.onMouseLeave?.(event); }}
      onMouseDown={(event) => { setPress(true); rest.onMouseDown?.(event); }}
      onMouseUp={(event) => { setPress(false); rest.onMouseUp?.(event); }}
      style={{
        display: "inline-flex", alignItems: "center", justifyContent: "center",
        width: box, height: box, padding: 0, background: bg,
        color: fill ? "#ffffff" : active ? "var(--text-brand)" : "var(--text-secondary)",
        border: fill ? `1px solid ${bg}` : variant === "solid" ? "1px solid var(--border-default)" : "1px solid transparent",
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
