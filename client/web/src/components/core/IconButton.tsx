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
  disabled?: boolean;
  style?: CSSProperties;
}

type IconButtonSize = NonNullable<IconButtonProps["size"]>;

const BOX: Record<IconButtonSize, number> = { sm: 28, md: 34, lg: 44 };

export function IconButton({ icon, label, size = "md", variant = "ghost", active = false, disabled = false, style = {}, ...rest }: IconButtonProps) {
  const [hover, setHover] = useState(false);
  const [press, setPress] = useState(false);
  const box = BOX[size] || BOX.md;
  const bg = active ? "var(--accent-quiet)" : hover && !disabled ? "var(--surface-quiet)" : variant === "solid" ? "var(--surface-card)" : "transparent";
  return (
    <button type="button" aria-label={label} title={label} disabled={disabled}
      onMouseEnter={() => setHover(true)} onMouseLeave={() => { setHover(false); setPress(false); }}
      onMouseDown={() => setPress(true)} onMouseUp={() => setPress(false)}
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
      }} {...rest}>
      <Icon name={icon} size={size === "lg" ? 20 : size === "sm" ? 15 : 18} />
    </button>
  );
}
