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
//
// The two skins below are that rule's one honest exception. The frontier's
// columns collapse *vertically* and keep a full-width labelled header to sit
// in; the aside collapses away entirely, so its control is a square in the
// panel's corner with no header row to be full-width inside — and its shut
// half is not here at all, but a `?` `IconButton` in `shell/Header.tsx`. So
// the shapes differ — but they stay in this file, share `SECTION_TOGGLE_HOVER`,
// and are both `ControlButton`, which is the part worth keeping single.

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

/** The shut control Now's standing-questions aside wears in its top-right
 * corner. No open/shut variant: shut, the panel is not on screen at all and
 * the `?` that reopens it is an `IconButton` in the shell's header, so this
 * skin only ever draws one state. `--row-height` is its box for the reason it
 * is a row's height — it is also the minimum touch target. */
export const ASIDE_TOGGLE_STYLE: CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  flex: "0 0 auto",
  width: "var(--row-height)",
  height: "var(--row-height)",
  padding: 0,
  borderRadius: "var(--radius-control)",
  background: "none",
  border: "none",
  cursor: "pointer",
  color: "var(--text-muted)",
};

export const SECTION_TOGGLE_HOVER: CSSProperties = {
  background: "var(--surface-quiet)",
  color: "var(--text-primary)",
};
