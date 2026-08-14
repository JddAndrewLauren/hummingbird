// The global capture focus hotkey (issue #110/S12, decided in #107's
// comment: "the hotkey belongs at shell level ... `src/App.tsx` is now that
// level"). This module is deliberately DOM-free — the caller (App.tsx)
// extracts the few facts below from a real `KeyboardEvent` at call time, so
// the matching rule itself stays a plain function a node-environment vitest
// test can execute (the same split `worker/*` already uses for its own
// pure-logic modules).
//
// "c" (no modifier), the same mnemonic GitHub and other capture-first tools
// use, and never while a control that already accepts typed input has
// focus — a hotkey that steals "c" out of a text field the person is
// already typing into would be worse than no hotkey at all.
/** The DOM id `CaptureBox`'s capture `Input` renders with — shared here
 * (rather than duplicated at both call sites) so the shell's hotkey and the
 * component that owns the actual `<input>` can never drift apart. Exactly one
 * box exists at a time (it lives in `shell/CapturePopover.tsx`), which is what
 * keeps a document-wide id honest. The value still reads `triage-` for the
 * stage a capture is born into by default; renaming it would only churn. */
export const CAPTURE_INPUT_ID = "triage-capture-input";

export interface CaptureHotkeyInput {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  /** Whether the event's target is already an editable control (an input,
   * textarea, select, or a `contenteditable` element). */
  targetIsEditable: boolean;
  /** The native `KeyboardEvent.isComposing` — set while an IME composition
   * (e.g. typing Japanese/Chinese/Korean) is in progress. A composition can
   * fire `keydown` events for keys that never reach `key`'s plain meaning,
   * "c" included, so this must hold the hotkey off the same way it holds
   * off a plain Enter-to-submit (round-2 review of PR #206). */
  isComposing: boolean;
}

export function isCaptureHotkey(input: CaptureHotkeyInput): boolean {
  if (input.isComposing) {
    return false;
  }
  if (input.ctrlKey || input.metaKey || input.altKey) {
    return false;
  }
  if (input.targetIsEditable) {
    return false;
  }
  return input.key.toLowerCase() === "c";
}

/** Whether this keystroke should close the open item detail (#404's Escape).
 *
 * Same DOM-free split as the hotkey above, and a second reader for the same
 * listener. The interesting part is `captureOpen`: `CapturePopover` binds its
 * own document listener and owns Escape while it is open, and it sits *over*
 * the detail panel — so one Escape must close the shallowest open thing only,
 * never both. One document listener cannot see another's intent, so the rule
 * is written against the state instead.
 *
 * `isComposing` for the same reason it holds off the hotkey: an Escape that
 * cancels an IME composition belongs to the composition.
 *
 * `targetIsEditable` is deliberately NOT consulted. Escape is not a character;
 * a reader who has just typed in the panel's Edit fields and presses Escape
 * means the panel, and no editable control here treats Escape as its own.
 */
export interface ItemDetailEscapeInput {
  key: string;
  isComposing: boolean;
  /** Whether the capture popover is open, and therefore owns this Escape. */
  captureOpen: boolean;
  /** Whether any item detail is open to close. */
  itemDetailOpen: boolean;
}

export function closesItemDetail(input: ItemDetailEscapeInput): boolean {
  return (
    input.key === "Escape" &&
    !input.isComposing &&
    !input.captureOpen &&
    input.itemDetailOpen
  );
}
