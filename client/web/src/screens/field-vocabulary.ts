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
// **Not the capture box's sliders.** Those are indexed by *position* rather
// than by value — `capture-meta.ts`'s `CAPTURE_SIZE_NAMES`/
// `CAPTURE_ENERGY_NAMES` own that correspondence, and its test pins it.
// They used to differ in wording too, displaying "normal" where the wire said
// `short`; ADR-0024 made those the same word, so position is now the only
// thing separating the two representations.
//
// That leaves three unlinked copies of the size vocabulary in this directory —
// these options, `capture-meta.ts`'s names, and `size-energy.ts`'s level map —
// and nothing mechanical holds them together. That is not hypothetical: when
// ADR-0024 renamed the middle size, this file was the copy that did not get
// renamed, and because the server still accepts `short` as a serde alias it
// went on writing a dead value successfully while displaying the old word.
// `field-vocabulary.test.ts` is the assertion that would have caught it.

import type { TaskItemDTO } from "../store/protocol";

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

/** A `Select` option whose value is a level of `T`, or `""` for "not set".
 *
 * The type parameter is the whole point. These lists were `{ value: string }`,
 * which is what let ADR-0024's rename land everywhere else and leave `"short"`
 * sitting here compiling perfectly — the server takes `short` as a serde
 * alias, so it kept writing, and only the word on screen was wrong. Anchoring
 * the value to `TaskItemDTO`'s own union makes that a build error instead of a
 * silent one. */
type LevelOption<T extends string> = { value: T | ""; label: string };

export const SIZE_OPTIONS: Array<LevelOption<NonNullable<TaskItemDTO["size"]>>> = [
  { value: "", label: "Not set" },
  { value: "quick", label: "Quick" },
  // ADR-0024's middle size. It was `short` on the wire until that decision.
  { value: "normal", label: "Normal" },
  { value: "deep", label: "Deep" },
];

export const ENERGY_OPTIONS: Array<LevelOption<NonNullable<TaskItemDTO["energy"]>>> = [
  { value: "", label: "Not set" },
  { value: "low", label: "Low" },
  { value: "medium", label: "Medium" },
  { value: "high", label: "High" },
];
