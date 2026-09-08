// What the icon set does not have. Two families — **size** as depth rings
// and **energy** as ascending bars (#446, ADR-0024) — because Lucide has
// nothing for "how deep is this work" or "what will it cost me", which are
// this product's questions and nobody else's. Then one mark, **Dropbox**
// (ADR-0036), for the opposite reason: Lucide dropped its brand glyphs, and
// that one is a vendor's, not ours. Lucide covers every other name in
// `ICON_MAP`.
//
// **Same call signature as `lucide-react`.** Every component here takes
// `{ size, strokeWidth, ...svgProps }`, draws in `currentColor` and lives in
// a 24 viewBox, so `ICON_MAP` stays a homogeneous map of glyph components
// and `Icon` needs no branch for these. That sameness is the whole design:
// the moment one of them needed special handling at the call site, five
// surfaces would start drifting.
//
// **`strokeWidth` is accepted and ignored.** The energy bars and the Dropbox
// mark carry no stroke at all, and the size rings are fixed at 2.5. The level is drawn by opacity
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
// not re-derived. `DropboxMark` carries its own geometry note, for the same
// reason: its numbers were measured, and re-deriving them loses the why.

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

/** The Dropbox mark, for ADR-0036's file links.
 *
 * **Why it is drawn here rather than named in `ICON_MAP`.** Lucide dropped
 * its brand glyphs, which `Icon.tsx` already records where the GitHub
 * workflow question falls back to `git-branch`. A bin day or a workflow
 * survives a generic stand-in; a button that says only "Add" plus a glyph
 * does not — the glyph *is* the noun, so `box` or `cloud` would leave the
 * control naming nothing in particular. This is the one place the vendor's
 * own mark is worth carrying, exactly as `dropbox/file-link.ts` is the one
 * place its path rules are.
 *
 * **Five rhombi, and it is filled.** The geometry is the mark's own,
 * scaled by 0.86 about the centre of the 24 box. Both halves of that were
 * measured rather than chosen (2026-09-08, rendered at 13/16/18/24/44 in
 * both themes):
 *
 *  - *Filled, not stroked.* Five outlined rhombi at 1.75 collapse into a
 *    smudge by 18px, and 13/16/18 are the three sizes this glyph is drawn
 *    at. The energy bars above are already filled, so this breaks no rule
 *    the file did not already have.
 *  - *0.86, not full size.* A filled shape carries more ink than an outline
 *    of the same box, and at full size this out-weighed the `link` and
 *    `notebook-text` glyphs sharing its row. 0.86 is where the three match.
 *
 * Trademark: nominative use — it points at the operator's own Dropbox
 * files and claims nothing about who wrote this app.
 */
export function DropboxMark(props: GlyphProps) {
  const { size = 24, strokeWidth, ...svg } = props;
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
      <path d="M6.84 3.234L12.001 6.521L6.84 9.808L1.68 6.521Z M17.16 3.234L22.32 6.521L17.16 9.808L12 6.521Z M6.84 9.809L12.001 13.096L6.84 16.383L1.68 13.096Z M17.16 9.809L22.32 13.096L17.16 16.383L12 13.096Z M12 14.192L17.16 17.479L12.001 20.766L6.84 17.479Z" />
    </svg>
  );
}
