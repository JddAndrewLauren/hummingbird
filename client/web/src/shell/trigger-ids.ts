// The DOM ids the two shell overlays' triggers carry. Two string constants
// in a file of their own, for the reason `nav-bar.ts` and `screens.ts` are
// also in `tsconfig.node.json`'s include list: `visual/surfaces.spec.ts` has
// to address these buttons, and that project has no JSX, so it cannot import
// anything from a `.tsx` — which is where both of these used to live. A spec
// that spelled the ids out instead would be a second copy of a value whose
// whole job is to be identical in two places.
//
// Why ids at all rather than refs or accessible names: the design system's
// `Button`/`IconButton` forward no ref, so a popover measuring the button it
// hangs from has nothing else to reach for (`CapturePopover.tsx`'s
// `useLayoutEffect`), and the hotkey paths hold no React handle on the
// trigger at all. Names are no help either, now that every Recall trigger
// wears the same one ("Search everything" — CONTEXT.md).

/** The id on the header's New button, measured by `CapturePopover`. */
export const CAPTURE_TRIGGER_ID = "shell-capture-trigger";

/** The id on the header's Search button, measured by `RecallOverlay`. */
export const RECALL_TRIGGER_ID = "shell-recall-trigger";
