import { useState } from "react";
import type { CSSProperties, HTMLAttributes, ReactNode } from "react";

/**
 * What a Card may render as: container elements only. Void elements
 * (`input`, `img`, `br`) take no children and throw at render, so they are
 * not in the contract.
 */
export type CardTag =
  | "div" | "section" | "article" | "aside" | "nav" | "header" | "footer"
  | "main" | "form" | "fieldset" | "li" | "a" | "button";

export interface CardProps extends Omit<HTMLAttributes<HTMLElement>, "style"> {
  /** 0 flat · 1 resting (default) · 2 raised · 3 floating (dialogs, menus). */
  elevation?: 0 | 1 | 2 | 3;
  padding?: string;
  /** Adds hover lift + pointer; use for whole-card click targets. */
  interactive?: boolean;
  /** Brand-tinted border, for the one card that is the answer on screen. */
  accent?: boolean;
  as?: CardTag;
  style?: CSSProperties;
  children?: ReactNode;
}

export function Card({ elevation = 1, padding = "var(--space-6)", interactive = false, accent = false, as: Tag = "div", onMouseEnter, onMouseLeave, style = {}, children, ...rest }: CardProps) {
  const [hover, setHover] = useState(false);
  const shadow = ["var(--shadow-0)", "var(--shadow-1)", "var(--shadow-2)", "var(--shadow-3)"][elevation] || "var(--shadow-1)";
  return (
    <Tag
      onMouseEnter={(event) => { if (interactive) { setHover(true); } onMouseEnter?.(event); }}
      onMouseLeave={(event) => { if (interactive) { setHover(false); } onMouseLeave?.(event); }}
      style={{
        background: "var(--surface-card)", padding,
        border: `1px solid ${accent ? "var(--accent-quiet-border)" : "var(--border-subtle)"}`,
        borderRadius: "var(--radius-card)",
        boxShadow: interactive && hover ? "var(--shadow-2)" : shadow,
        transform: interactive && hover ? "translateY(var(--lift-hover))" : "none",
        transition: "box-shadow var(--dur-base) var(--ease-flit), transform var(--dur-base) var(--ease-hover), border-color var(--dur-fast) var(--ease-flit)",
        cursor: interactive ? "pointer" : undefined,
        ...style,
      }} {...rest}>
      {children}
    </Tag>
  );
}
