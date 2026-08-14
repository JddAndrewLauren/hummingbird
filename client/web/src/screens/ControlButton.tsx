// A control that honours the design system's hover contract.
//
// These stay hand-rolled rather than becoming `components/core/Button`: every
// one of them is a *toggle* carrying `aria-pressed` or `aria-expanded`, and the
// section headers are full-width buttons inside an `h2` whose own font and left
// alignment the library's centred fixed-size skins do not fit. What they must
// not do is skip the hover state — "a hovered thing gets *more* solid, not
// less" — which is the only thing this wrapper adds.
//
// It lives here rather than inside `FrontierColumns.tsx` because Now's
// standing-questions aside collapses through the same gesture and must not grow
// a second, subtly different toggle to do it.

import { useState } from "react";
import type { ButtonHTMLAttributes, CSSProperties } from "react";

export function ControlButton({
  baseStyle,
  hoverStyle,
  children,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  baseStyle: CSSProperties;
  hoverStyle: CSSProperties;
}) {
  const [hover, setHover] = useState(false);
  return (
    <button
      {...rest}
      type="button"
      onMouseEnter={(event) => {
        setHover(true);
        rest.onMouseEnter?.(event);
      }}
      onMouseLeave={(event) => {
        setHover(false);
        rest.onMouseLeave?.(event);
      }}
      style={{
        transition:
          "background var(--dur-fast) var(--ease-flit), color var(--dur-fast) var(--ease-flit)",
        ...baseStyle,
        ...(hover ? hoverStyle : null),
      }}
    >
      {children}
    </button>
  );
}

/** The collapse control a section header wears: a full-width, left-aligned
 * button carrying a rotating chevron. Shared by the frontier's columns and
 * Now's standing-questions aside so the two collapse identically. */
export function sectionToggleStyle(collapsed: boolean): CSSProperties {
  return {
    display: "flex",
    alignItems: "center",
    gap: "var(--space-3)",
    width: "100%",
    minHeight: "var(--row-height)",
    padding: "var(--space-2)",
    borderRadius: "var(--radius-control)",
    background: "none",
    border: "none",
    textAlign: "left",
    cursor: "pointer",
    font: "var(--type-body-strong)",
    color: collapsed ? "var(--text-secondary)" : "var(--text-primary)",
  };
}

export const SECTION_TOGGLE_HOVER: CSSProperties = {
  background: "var(--surface-quiet)",
  color: "var(--text-primary)",
};
