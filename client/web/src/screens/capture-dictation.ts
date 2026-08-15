// #379's pure half: where a dictated transcript lands in the capture draft.
// A node-tested `.ts` beside `capture-validation.ts` and `capture-meta.ts`,
// following that split — the deciding logic is here, `CaptureBox.tsx` only
// wires it to a recognizer and a caret.
//
// The whole design rests on one property: **totality**. The draft is frozen
// into two halves when a session starts, and every transcript that arrives is
// spliced between those same two halves. So the same frozen draft plus the
// same transcript always gives the same string, which is what makes a later
// interim result REPLACE the earlier one in the field instead of
// accumulating — no diffing, no bookkeeping of what was inserted last, and
// nothing to get wrong when a result goes interim -> final and back.

/** A capture draft split at the caret, held for the length of one dictation
 * session. `prefix + suffix` is what the field would say with the transcript
 * empty — a selection's contents are NOT in either half, because a dictation
 * over a selection replaces it, the same thing typing over a selection does.
 * `selected` carries those contents anyway (#380): a splice never reads it,
 * but a session that ends up cancelled (`restoreDraft`, below) needs the
 * words back, byte-for-byte, not just the halves either side of them. */
export interface FrozenDraft {
  prefix: string;
  suffix: string;
  selected: string;
}

export interface SplicedDraft {
  value: string;
  /** Where the caret belongs afterwards: the end of the inserted text, so
   * carrying on typing continues the sentence that was just dictated. */
  caret: number;
}

/** Never insert a space directly before one of these — dictation that lands
 * in front of existing punctuation should read as though it were typed. */
const NO_SPACE_BEFORE = /^[,.?!]/;

/** Freezes `draft` at the caret. `selectionStart`/`selectionEnd` are the
 * field's own values, so `null` (an unfocused field, or a field type that
 * reports no selection) means "append" rather than "insert at 0" — an
 * unfocused field has no caret to insert at, and dropping a transcript in
 * front of an existing draft would be a surprise.
 *
 * Out-of-range and reversed values are clamped and ordered rather than
 * rejected: this is a DOM reading, and a total function over it is one less
 * state for the caller to hold. */
export function freezeDraft(
  draft: string,
  selectionStart: number | null,
  selectionEnd: number | null,
): FrozenDraft {
  if (selectionStart === null || selectionEnd === null) {
    return { prefix: draft, suffix: "", selected: "" };
  }
  const first = clamp(Math.min(selectionStart, selectionEnd), draft.length);
  const last = clamp(Math.max(selectionStart, selectionEnd), draft.length);
  return { prefix: draft.slice(0, first), suffix: draft.slice(last), selected: draft.slice(first, last) };
}

function clamp(index: number, length: number): number {
  if (!Number.isFinite(index) || index < 0) {
    return 0;
  }
  return Math.min(Math.trunc(index), length);
}

/** Splices `transcript` between the frozen halves, adding **at most one**
 * space on each side and none where the text already provides one — never a
 * double space, never a space before `,.?!`.
 *
 * A transcript that is empty or all whitespace inserts nothing at all, so a
 * session that heard nothing leaves the draft exactly as it was (and an empty
 * draft still empty, which keeps `canSubmitCapture` false). */
export function spliceTranscript(frozen: FrozenDraft, transcript: string): SplicedDraft {
  const { prefix, suffix } = frozen;
  const spoken = transcript.trim();
  if (spoken === "") {
    return { value: prefix + suffix, caret: prefix.length };
  }
  const lead = prefix !== "" && !/\s$/.test(prefix) ? " " : "";
  const trail = suffix !== "" && !/^\s/.test(suffix) && !NO_SPACE_BEFORE.test(suffix) ? " " : "";
  const head = prefix + lead + spoken;
  return { value: head + trail + suffix, caret: head.length };
}

export interface RestoredDraft {
  value: string;
  selectionStart: number;
  selectionEnd: number;
}

/** Undoes a session with no spacing rules of its own: `prefix + selected +
 * suffix` is byte-for-byte the string `freezeDraft` split, and the caret goes
 * back to the exact range that was selected (collapsed to a caret when there
 * was none) — never the "no transcript" case `spliceTranscript` handles,
 * which drops `selected` on the floor because a live session's own splice is
 * built to replace a selection, not restore it. */
export function restoreDraft(frozen: FrozenDraft): RestoredDraft {
  const { prefix, suffix, selected } = frozen;
  return {
    value: prefix + selected + suffix,
    selectionStart: prefix.length,
    selectionEnd: prefix.length + selected.length,
  };
}
