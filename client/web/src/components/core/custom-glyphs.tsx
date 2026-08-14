// The two glyph families the icon set does not have: **size** as depth
// rings and **energy** as ascending bars (#446, ADR-0024). Lucide covers
// every other name in `ICON_MAP`; it has nothing for "how deep is this work"
// or "what will it cost me", because those are this product's questions.
//
// **Same call signature as `lucide-react`.** Every component here takes
// `{ size, strokeWidth, ...svgProps }`, draws in `currentColor` and lives in
// a 24 viewBox, so `ICON_MAP` stays a homogeneous map of glyph components
// and `Icon` needs no branch for these. That sameness is the whole design:
// the moment one of them needed special handling at the call site, five
// surfaces would start drifting.
//
// **`strokeWidth` is accepted and ignored.** The energy bars carry no stroke
// at all, and the size rings are fixed at 2.5. The level is drawn by opacity
// against a constant weight, so honouring a caller's stroke would thicken
// the ghosted rings along with the earned ones and break the family's
// optical match with the Lucide glyphs beside it. Accepting the prop and
// dropping it is the price of the homogeneous signature above.
//
// **The ramp is opacity, and it is not the colour.** Earned elements draw at
// 1, unearned at 0.25, and the unset variant draws everything at 0.45 — a
// legible ghost, never a warning. Colour arrives separately, from
// `screens/size-energy.ts`, and always on the icon *and* its label together
// (design README, ICONOGRAPHY: icons never carry colour independently of
// their label).
//
// Geometry is verbatim from the #446 design handoff — the radii, the bar
// rects, the opacities and the 2.5 stroke are marked final there. Copied,
// not re-derived.

import type { SVGProps } from "react";

interface GlyphProps extends Omit<SVGProps<SVGSVGElement>, "ref"> {
  size?: number | string;
  /** Accepted for signature parity with `lucide-react`, deliberately unused
   * — see the module header. */
  strokeWidth?: number | string;
}

/** The three opacity stops the two families share, by position on the
 * scale rather than by name: unearned, earned, and the flat unset wash. */
const UNEARNED = 0.25;
const EARNED = 1;
const UNSET = 0.45;

function SizeRings(props: GlyphProps & { dot: number; inner: number; outer: number }) {
  const { size = 24, strokeWidth, dot, inner, outer, ...svg } = props;
  void strokeWidth; // accepted for `lucide-react` parity, never applied
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2.5}
      {...svg}
    >
      <circle cx="12" cy="12" r="3" fill="currentColor" stroke="none" opacity={dot} />
      <circle cx="12" cy="12" r="6.75" opacity={inner} />
      <circle cx="12" cy="12" r="10.5" opacity={outer} />
    </svg>
  );
}

function EnergyBars(props: GlyphProps & { first: number; second: number; third: number }) {
  const { size = 24, strokeWidth, first, second, third, ...svg } = props;
  void strokeWidth; // accepted for `lucide-react` parity, never applied
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="currentColor"
      stroke="none"
      {...svg}
    >
      <rect x="4" y="14" width="4.5" height="6" rx="2" opacity={first} />
      <rect x="9.75" y="9" width="4.5" height="11" rx="2" opacity={second} />
      <rect x="15.5" y="4" width="4.5" height="16" rx="2" opacity={third} />
    </svg>
  );
}

export function SizeUnset(props: GlyphProps) {
  return <SizeRings dot={UNSET} inner={UNSET} outer={UNSET} {...props} />;
}

export function SizeQuick(props: GlyphProps) {
  return <SizeRings dot={EARNED} inner={UNEARNED} outer={UNEARNED} {...props} />;
}

export function SizeNormal(props: GlyphProps) {
  return <SizeRings dot={EARNED} inner={EARNED} outer={UNEARNED} {...props} />;
}

export function SizeDeep(props: GlyphProps) {
  return <SizeRings dot={EARNED} inner={EARNED} outer={EARNED} {...props} />;
}

export function EnergyUnset(props: GlyphProps) {
  return <EnergyBars first={UNSET} second={UNSET} third={UNSET} {...props} />;
}

export function EnergyLow(props: GlyphProps) {
  return <EnergyBars first={EARNED} second={UNEARNED} third={UNEARNED} {...props} />;
}

export function EnergyMedium(props: GlyphProps) {
  return <EnergyBars first={EARNED} second={EARNED} third={UNEARNED} {...props} />;
}

export function EnergyHigh(props: GlyphProps) {
  return <EnergyBars first={EARNED} second={EARNED} third={EARNED} {...props} />;
}
