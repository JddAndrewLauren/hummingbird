import { useState } from "react";
import type { CSSProperties, ElementType, HTMLAttributes, JSX, ReactNode } from "react";

export interface CardProps extends Omit<HTMLAttributes<HTMLElement>, "style"> {
  /** 0 flat · 1 resting (default) · 2 raised · 3 floating (dialogs, menus). */
  elevation?: 0 | 1 | 2 | 3;
  padding?: string;
  /** Adds hover lift + pointer; use for whole-card click targets. */
  interactive?: boolean;
  /** Brand-tinted border, for the one card that is the answer on screen. */
  accent?: boolean;
  as?: keyof JSX.IntrinsicElements;
  style?: CSSProperties;
  children?: ReactNode;
}

export function Card({ elevation = 1, padding = "var(--space-6)", interactive = false, accent = false, as = "div", style = {}, children, ...rest }: CardProps) {
  const [hover, setHover] = useState(false);
  // `as` keeps the design contract's `keyof JSX.IntrinsicElements`, but TS can't
  // render the full intrinsic union: its SVG members reject the HTMLAttributes
  // rest props. Narrow to the HTML-compatible subset for the JSX site only —
  // a cast, because the contract's SVG tags (e.g. "symbol") are outside it.
  const Tag = as as ElementType<HTMLAttributes<HTMLElement>>;
  const shadow = ["var(--shadow-0)", "var(--shadow-1)", "var(--shadow-2)", "var(--shadow-3)"][elevation] || "var(--shadow-1)";
  return (
    <Tag
      onMouseEnter={interactive ? () => setHover(true) : undefined}
      onMouseLeave={interactive ? () => setHover(false) : undefined}
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
