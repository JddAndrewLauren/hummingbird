// THROWAWAY (#801, Phase 1). The floating switcher.
//
// Deliberately ugly against the brand — a high-contrast capsule pinned to the
// bottom of the window — so that nothing in it is ever mistaken for part of
// the design being judged. It is a capsule rather than a fade-out strip
// because that is the one thing the design system does say about floating
// chrome ("capsules, always").
//
// It carries the prototype's whole state readout, which is the `/prototype`
// rule the variants would otherwise fail: after every action, say what
// happened. `last:` prints the item by its title and the field the drop
// actually wrote — never `HB-<seq>`, which no surface displays and the
// operator cannot look up.

import { useEffect } from "react";
import {
  PHYSICS_LABEL,
  PHYSICS_MODES,
  VARIANTS,
  VARIANT_LABEL,
  type BarState,
  type PhysicsMode,
  type VariantKey,
} from "./seam";

const CHIP: React.CSSProperties = {
  font: "var(--type-meta)",
  textTransform: "uppercase",
  letterSpacing: "0.08em",
  padding: "var(--space-2) var(--space-4)",
  borderRadius: "var(--radius-pill)",
  border: "1px solid var(--border-strong)",
  background: "transparent",
  color: "var(--text-secondary)",
  cursor: "pointer",
};

const CHIP_ON: React.CSSProperties = {
  ...CHIP,
  background: "var(--accent)",
  borderColor: "var(--accent)",
  color: "var(--on-accent)",
};

export function PrototypeBar(state: BarState) {
  const { variant, mode, pick, pickMode } = state;

  // Arrow keys cycle, unless the reader is typing — the capture box and the
  // filter inputs are on this screen.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target;
      if (
        target instanceof HTMLElement &&
        (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable)
      ) {
        return;
      }
      const at = VARIANTS.indexOf(variant);
      if (event.key === "ArrowRight") pick(VARIANTS[(at + 1) % VARIANTS.length] as VariantKey);
      if (event.key === "ArrowLeft") pick(VARIANTS[(at - 1 + VARIANTS.length) % VARIANTS.length] as VariantKey);
      const digit = Number(event.key);
      if (variant === "D" && digit >= 1 && digit <= PHYSICS_MODES.length) {
        pickMode(PHYSICS_MODES[digit - 1] as PhysicsMode);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [variant, pick, pickMode]);

  return (
    <div
      style={{
        position: "fixed",
        left: "50%",
        bottom: "var(--space-6)",
        transform: "translateX(-50%)",
        zIndex: 80,
        maxWidth: "min(880px, calc(100vw - var(--space-12)))",
        display: "flex",
        flexDirection: "column",
        gap: "var(--space-3)",
        padding: "var(--space-4) var(--space-6)",
        background: "var(--surface-card)",
        border: "1px solid var(--border-strong)",
        borderRadius: "var(--radius-sheet)",
        boxShadow: "var(--shadow-3)",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: "var(--space-4)" }}>
        <button type="button" style={CHIP} onClick={() => pick(prev(variant))} aria-label="Previous variant">
          {"<"}
        </button>
        <span style={{ font: "var(--type-body)", flex: 1, minWidth: 0 }}>
          <strong>{variant}</strong> — {VARIANT_LABEL[variant]}
        </span>
        <button type="button" style={CHIP} onClick={() => pick(next(variant))} aria-label="Next variant">
          {">"}
        </button>
      </div>

      {variant === "D" ? (
        <div style={{ display: "flex", alignItems: "center", gap: "var(--space-3)", flexWrap: "wrap" }}>
          {PHYSICS_MODES.map((m, index) => (
            <button
              key={m}
              type="button"
              style={m === mode ? CHIP_ON : CHIP}
              onClick={() => pickMode(m)}
              aria-pressed={m === mode}
            >
              {index + 1} {m}
            </button>
          ))}
          <span className="hb-meta" style={{ flexBasis: "100%" }}>
            {PHYSICS_LABEL[mode]}
          </span>
        </div>
      ) : null}

      <div style={{ display: "flex", alignItems: "baseline", gap: "var(--space-4)" }}>
        <span className="hb-meta" style={{ flex: 1, minWidth: 0 }}>
          {state.lastMove === null ? "no moves yet" : `last: ${state.lastMove}`}
        </span>
        <span className="hb-meta">{state.moves} moved</span>
        <button type="button" style={CHIP} onClick={state.reset}>
          Reset
        </button>
      </div>
    </div>
  );
}

function next(variant: VariantKey): VariantKey {
  return VARIANTS[(VARIANTS.indexOf(variant) + 1) % VARIANTS.length] as VariantKey;
}

function prev(variant: VariantKey): VariantKey {
  return VARIANTS[(VARIANTS.indexOf(variant) - 1 + VARIANTS.length) % VARIANTS.length] as VariantKey;
}
