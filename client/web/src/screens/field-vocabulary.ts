// The option lists the item forms offer, in one place.
//
// Free vocabulary in the schema (`items.context`), a fixed list here: this is
// a personal system and these are the places its owner actually works. Size
// and Energy are `hummingbird_domain`'s own closed vocabularies, spelled with
// the wire's names as values and sentence-case labels.
//
// One module because two forms offer the same choices — the capture box and
// the item editor — and a context added to one copy and not the other is a
// list that quietly disagrees with itself depending on where you sort from.
//
// **Not the capture box's sliders.** Those carry display labels that are
// deliberately *not* the domain vocabulary ("normal" for `short`), and they
// are indexed by position rather than by value — `capture-meta.ts` owns that
// correspondence and its test pins it.

export const CONTEXTS = [
  "@home",
  "@computer",
  "@phone",
  "@errands",
  "@garden",
  "@waiting",
] as const;

/** The `Select` options for context, with the resting "Not set" first. */
export const CONTEXT_OPTIONS: Array<{ value: string; label: string }> = [
  { value: "", label: "Not set" },
  ...CONTEXTS.map((context) => ({ value: context, label: context })),
];

// The middle size is `normal`, not `short` (ADR-0024). The server still
// accepts `short` as a serde alias, so the old value would have gone on
// writing successfully while displaying a word the vocabulary no longer has —
// which is why this is a merge fix and not a rename that could have waited.
export const SIZE_OPTIONS: Array<{ value: string; label: string }> = [
  { value: "", label: "Not set" },
  { value: "quick", label: "Quick" },
  { value: "normal", label: "Normal" },
  { value: "deep", label: "Deep" },
];

export const ENERGY_OPTIONS: Array<{ value: string; label: string }> = [
  { value: "", label: "Not set" },
  { value: "low", label: "Low" },
  { value: "medium", label: "Medium" },
  { value: "high", label: "High" },
];
