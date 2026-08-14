// How an item's **size** and **energy** are drawn, in one place (#446,
// ADR-0024). Five surfaces render these two dimensions — the detail panel,
// the row, the frontier card, the top-pick card and the capture sliders —
// and before this module each of them printed the raw wire word in mono
// text. A glyph and a colour per level is more to get right than a word is,
// so it is decided once here rather than five times at the call sites.
//
// Pure, no JSX: the mapping is the part with a wrong answer available, and
// this way it is unit-testable without a DOM. The drawing itself is
// `components/core/custom-glyphs.tsx`; `Icon` is what puts the two together.
//
// **Icon and label always share a colour.** The design system's rule
// (README, ICONOGRAPHY: "icons never carry colour independently of their
// label") is what makes `levelColor` a single answer used for both, rather
// than a tint applied to the glyph alone. A coloured mark beside an
// uncoloured word reads as decoration; coloured together they read as one
// statement about the item.
//
// **Unset is a resting state, not a warning.** Nothing here escalates when
// a dimension is absent: the glyph ghosts to a flat 45% wash, the label is
// an em dash and the colour is `--text-muted`. An item nobody has judged is
// not an item with a problem — CONTEXT.md is explicit that an absent size is
// an unmade judgement rather than a claim of smallness.

import type { IconName } from "../components/core/Icon";
import type { TaskItemDTO } from "../store/protocol";

type Size = TaskItemDTO["size"];
type Energy = TaskItemDTO["energy"];

/** The ramp, by **position on the scale** rather than by name — which is
 * what lets size and energy share one table instead of keeping two that can
 * drift. Position 0 is the unset resting state; 1, 2 and 3 are the three
 * levels, lightest first.
 *
 * Every token already exists in `design/tokens/colors.css` at both themes,
 * so this adds no colour to the system: it borrows the urgency pair for the
 * top two steps. That borrowing is the thing ADR-0024 decision 2 narrows
 * ADR-0021 over — on a frontier card an amber mark can now mean *due soon*
 * or *normal size*, and the card's own colour is what still means urgency
 * and nothing else. */
const RAMP = [
  "var(--text-muted)",
  "var(--status-done-fg)",
  "var(--urgency-soon)",
  "var(--urgency-now)",
] as const;

/** Both vocabularies in one record, which is only possible because they
 * share no word. A level added to either dimension fails to compile here
 * until it is given a position — the ramp cannot silently acquire a step
 * with no colour. */
const POSITION: Record<NonNullable<Size> | NonNullable<Energy>, 1 | 2 | 3> = {
  quick: 1,
  normal: 2,
  deep: 3,
  low: 1,
  medium: 2,
  high: 3,
};

export function sizeIcon(size: Size): IconName {
  switch (size) {
    case "quick":
      return "size-quick";
    case "normal":
      return "size-normal";
    case "deep":
      return "size-deep";
    default:
      return "size-unset";
  }
}

export function energyIcon(energy: Energy): IconName {
  switch (energy) {
    case "low":
      return "energy-low";
    case "medium":
      return "energy-medium";
    case "high":
      return "energy-high";
    default:
      return "energy-unset";
  }
}

/** The CSS colour for one level of either dimension — for the glyph **and**
 * its label, never one without the other. */
export function levelColor(value: Size | Energy): string {
  return value ? RAMP[POSITION[value]] : RAMP[0];
}

/** The uppercase meta-line label. The em dash is what "unset" looks like —
 * the same width in every row, and readable as a deliberate blank rather
 * than a missing render. */
export function sizeLabel(size: Size): string {
  return size ? size.toUpperCase() : "—";
}

export function energyLabel(energy: Energy): string {
  return energy ? energy.toUpperCase() : "—";
}
