// The global Recall focus hotkey (issue #480, decided in #477's plan comment
// 10: "a `/` plain-key hotkey following the `capture-hotkey.ts` pattern").
// Deliberately DOM-free, the identical split `capture-hotkey.ts` uses and for
// the same reason: the caller (`App.tsx`) extracts the few facts below from a
// real `KeyboardEvent` at call time, so the matching rule itself stays a
// plain function a node-environment vitest test can execute exhaustively.
//
// "/" (no modifier), the gesture people already expect from GitHub, Slack and
// most search-first apps — and never while a control that already accepts
// typed input has focus, for the identical reason `isCaptureHotkey` guards
// "c": a hotkey that steals "/" out of a text field the person is already
// typing into (where it is an ordinary character, not a command) would be
// worse than no hotkey at all.

export interface RecallHotkeyInput {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  /** Whether the event's target is already an editable control (an input,
   * textarea, select, or a `contenteditable` element) — `capture-hotkey.ts`'s
   * own field, computed the same way at the same call site. */
  targetIsEditable: boolean;
  /** The native `KeyboardEvent.isComposing` — set while an IME composition is
   * in progress. A composition can fire `keydown` events for keys that never
   * reach `key`'s plain meaning, "/" included, so this must hold the hotkey
   * off the same way it holds off "c" (`capture-hotkey.ts`'s own doc). */
  isComposing: boolean;
}

export function isRecallHotkey(input: RecallHotkeyInput): boolean {
  if (input.isComposing) {
    return false;
  }
  if (input.ctrlKey || input.metaKey || input.altKey) {
    return false;
  }
  if (input.targetIsEditable) {
    return false;
  }
  return input.key === "/";
}
