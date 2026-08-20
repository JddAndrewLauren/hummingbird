// Which suggestions a `Combobox` popup shows, as a plain function.
//
// The rule exists because the native `<datalist>` this control used to be
// had exactly one mode and no way out of it: Chromium filters its options
// case-insensitively by substring against whatever the input currently
// holds. That was survivable while the field was usually empty, and stopped
// being survivable in #641, which made capture's Context **sticky** across
// submits — so the box almost always holds a value, the popup almost always
// collapses to the one option matching it, and the rest of the vocabulary
// became unreachable.
//
// Hence `browseAll`: the mode the chevron opens in, where the query is
// ignored and the whole list shows regardless of what is in the box. That
// one boolean **is the fix**; everything else here reproduces the filtering
// Chromium already did, deliberately unchanged, so type-to-filter behaves
// exactly as it did before the control was rewritten.
//
// DOM-free and unit-tested on its own, the split `shell/escape-claimants.ts`
// and `shell/nav-bar.ts` already use: the decision is a pure function a
// node-environment test can execute exhaustively, and `Combobox.tsx` is left
// holding only markup and event plumbing.

/** The suggestions to render, in the caller's order.
 *
 * @param browseAll opened by the chevron (or by a keyboard arrow into a shut
 * list) — show everything, whatever the box holds. */
export function visibleSuggestions(
  suggestions: readonly string[],
  query: string,
  browseAll: boolean,
): string[] {
  const needle = query.trim().toLowerCase();
  if (browseAll || needle === "") {
    return [...suggestions];
  }
  return suggestions.filter((suggestion) => suggestion.toLowerCase().includes(needle));
}
