// Who owns an Escape, when more than one thing is open.
//
// The shell stacks overlays: the capture popover sits over everything, so
// does Recall's overlay (#480 — both are fixed chrome rendered as siblings of
// the same z-index, over any screen), the phone's More sheet over the screen,
// an item detail panel beside it. Each of them used to bind its own `keydown`
// on the document and close itself, and a document listener cannot see
// another's intent — so on a phone with a hardware keyboard, one Escape with
// both the sheet and an item open closed both, and the reader lost something
// they never asked to close.
//
// The fix is not a fourth guard bolted onto a third handler. It is this: one
// ordered list of claimants, one place that reads it, and no component owning
// Escape for itself any more. `App.tsx` holds every flag below and does the
// closing; `CapturePopover` and `NavBar` are controlled and bind nothing.
//
// **Adding a claimant is three edits, and the compiler names all three.**
// Put it in `ESCAPE_CLAIMANTS` at its depth, and both `EscapeInput["open"]`
// and `App.tsx`'s closer map stop type-checking until it is answered — they
// are `Record`s over this list, deliberately, not `Partial`s. A claimant that
// can be silently forgotten is the one outcome with no visible symptom
// (`nav-bar.ts` makes the same argument about its own partition).
//
// This module is DOM-free, the same split `capture-hotkey.ts` uses and for the
// same reason: the caller extracts the two facts below from a real
// `KeyboardEvent`, so the rule itself stays a plain function a node-environment
// vitest test can execute exhaustively.

/** Every overlay that can claim an Escape, **shallowest first** — which is
 * the whole rule. One Escape closes the topmost open thing and nothing else.
 *
 * `search` (#480, Recall's overlay) sits right after `capture`: the two are
 * the shell's only global, reachable-from-anywhere overlays, both fixed chrome
 * at the same z-index rendered as siblings in `App.tsx` — neither is scoped to
 * a screen or a device form the way `navSheet` (phone only) and `itemDetail`
 * (beside a screen's own content) are.
 *
 * The Grill takeover is deliberately absent: it claims no Escape at all
 * (`GrillTakeover.tsx` — leaving an interview is a deliberate Back), and it
 * replaces the centre column rather than covering anything. */
export const ESCAPE_CLAIMANTS = ["capture", "search", "navSheet", "itemDetail"] as const;

export type EscapeClaimant = (typeof ESCAPE_CLAIMANTS)[number];

export interface EscapeInput {
  key: string;
  /** The native `KeyboardEvent.isComposing` — set while an IME composition is
   * in progress. An Escape that cancels a composition belongs to the
   * composition, not to any overlay (`capture-hotkey.ts`'s same rule). */
  isComposing: boolean;
  /** Which claimants are open. Total over `ESCAPE_CLAIMANTS` on purpose: see
   * the header. */
  open: Readonly<Record<EscapeClaimant, boolean>>;
}

/** The claimant this keystroke belongs to, or `null` if it belongs to nobody.
 *
 * `targetIsEditable` is deliberately not a factor. Escape is not a character;
 * a reader who has just typed in the detail panel's Edit fields and presses
 * Escape means the panel, and no editable control in the shell treats Escape
 * as its own. */
export function escapeClaimant(input: EscapeInput): EscapeClaimant | null {
  if (input.key !== "Escape" || input.isComposing) {
    return null;
  }
  return ESCAPE_CLAIMANTS.find((claimant) => input.open[claimant]) ?? null;
}
