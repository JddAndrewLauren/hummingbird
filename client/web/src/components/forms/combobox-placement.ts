// Where a `Combobox`'s popup goes, as a plain function over measured boxes.
//
// It is `position: fixed` rather than absolute inside the control, which is
// the whole reason this module exists. The capture popover is a `maxHeight` +
// `overflowY: auto` dialog (`shell/CapturePopover.tsx`) and the item panel
// sits in a scrolling column; an absolutely-positioned popup inside either is
// clipped at that container's edge, which is exactly what the first capture
// of this surface photographed — a list of six contexts showing one and a
// half. A fixed element's containing block is the viewport, so no ancestor's
// `overflow` clips it, and the price is that its coordinates have to be
// computed rather than declared.
//
// Computed here rather than in the component for the usual reason
// (`escape-claimants.ts`, `frontier-lanes.ts`): it consumes measured pixels,
// which makes it untestable where it is used and trivially testable here.

/** The measured control — a `DOMRect`'s four relevant numbers, in viewport
 * coordinates, which is what `getBoundingClientRect` already returns. */
export interface FieldBox {
  top: number;
  bottom: number;
  left: number;
  width: number;
}

export interface PopupPlacement {
  /** Viewport coordinates, for `position: fixed`. */
  top: number;
  left: number;
  width: number;
  /** What the popup may grow to here — never past the viewport edge. */
  maxHeight: number;
  /** Whether it opened upward. The component does not act on this; it is
   * what makes the choice assertable. */
  above: boolean;
}

/** Below the field when there is room, above it when there is more room
 * there, and never taller than the side it landed on.
 *
 * @param gap the space between field and popup.
 * @param margin the space kept clear of the viewport's own edge.
 * @param preferredHeight the popup's natural cap; the result never exceeds it.
 */
export function popupPlacement(
  field: FieldBox,
  viewportHeight: number,
  { gap, margin, preferredHeight }: { gap: number; margin: number; preferredHeight: number },
): PopupPlacement {
  const roomBelow = viewportHeight - field.bottom - gap - margin;
  const roomAbove = field.top - gap - margin;
  // Ties and near-ties go downward: a list that opens under the field is
  // what everything else in this app's forms does, and flipping should take
  // a real reason.
  const above = roomBelow < preferredHeight && roomAbove > roomBelow;
  const maxHeight = Math.max(0, Math.min(preferredHeight, above ? roomAbove : roomBelow));
  return {
    top: above ? field.top - gap - maxHeight : field.bottom + gap,
    left: field.left,
    width: field.width,
    maxHeight,
    above,
  };
}
