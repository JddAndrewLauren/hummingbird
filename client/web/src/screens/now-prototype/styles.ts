// THROWAWAY (#801, Phase 1). The one styling vocabulary the four variants
// share, so they disagree about the *gesture* and not about what a lit column
// looks like — the comparison is worthless if each variant also invents its
// own colours.
//
// Straight off the design system (`/hummingbird-design`, and the tokens in
// `design/tokens/`), with the three rules that bite here:
//
//  - **The highlight goes on the column, never on the card.** ADR-0021
//    decision 2 (as narrowed by ADR-0024): the card's own colour means
//    urgency. A drop-target card tinted green would be a second meaning for
//    the same surface.
//  - **A hovered thing gets more solid, not less.** So a lit column takes a
//    `--surface-quiet` fill and a `--border-strong` outline; nothing anywhere
//    fades by opacity, and the receded source card in B goes quiet rather
//    than transparent.
//  - **Status colour means stage, tier or urgency.** So a refused column
//    (`overdue`) is not painted red: it simply never lights, and the cursor
//    says `not-allowed`. Refusal is the absence of the affordance.

import type { CSSProperties } from "react";

/** A column that would accept the card being carried. Written straight onto
 * the element during a gesture, so it must be a flat property bag. */
export const LIT: Record<string, string> = {
  background: "var(--surface-quiet)",
  outline: "1px solid var(--border-strong)",
  outlineOffset: "var(--space-2)",
  borderRadius: "var(--radius-card)",
};

export const UNLIT: Record<string, string> = {
  background: "",
  outline: "",
  outlineOffset: "",
  borderRadius: "",
};

/** Applied to every drawn part of a column at once — the chunk and every
 * continuation — because they are one column. */
export function paint(elements: readonly HTMLElement[], on: boolean): void {
  for (const el of elements) {
    Object.assign(el.style, on ? LIT : UNLIT);
    if (on) {
      el.style.transition = "background var(--dur-fast) var(--ease-flit)";
    }
  }
}

/** The same, as React style for the variants that re-render anyway (C). */
export const LIT_STYLE: CSSProperties = {
  ...LIT,
  transition: "background var(--dur-fast) var(--ease-flit)",
};

/** A card being carried: elevation 3 is the system's "floating" step, which
 * is what a dialog gets — and a card held above the board is exactly that.
 * `transition: none` because a rAF loop owns the transform now, and `Card`'s
 * own 200ms transform transition would fight it every frame. */
export const CARRIED: Record<string, string> = {
  position: "relative",
  zIndex: "40",
  boxShadow: "var(--shadow-3)",
  transition: "none",
  cursor: "grabbing",
  willChange: "transform",
};

export const DROPPED: Record<string, string> = {
  position: "",
  zIndex: "",
  boxShadow: "",
  transition: "",
  cursor: "",
  willChange: "",
  transform: "",
};

/** B's source slot: the card has left, and what stays behind recedes the way
 * the design system recedes anything — to `--surface-quiet`, not to 40%
 * opacity. */
export const VACATED: CSSProperties = {
  background: "var(--surface-quiet)",
  boxShadow: "none",
  color: "var(--text-muted)",
};

/** B's landing preview, and the mark on C's picked card: the accent border
 * the system reserves for "the one card that is the answer on screen" — a
 * tinted border and a quiet fill, never a solid accent block. */
export const LANDING: CSSProperties = {
  border: "1px solid var(--accent-quiet-border)",
  background: "var(--accent-quiet)",
  borderRadius: "var(--radius-card)",
};

/** Cards displaced to open a slot, and the flight's own settle. Not a token:
 * these are the durations D exists to argue with. */
export const PART_TRANSITION = "transform var(--dur-base) var(--ease-flit)";
