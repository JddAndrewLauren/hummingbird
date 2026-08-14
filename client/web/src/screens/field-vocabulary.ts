// The option lists the item forms offer, in one place.
//
// **Context is suggestions; Size and Energy are the vocabulary.** That is the
// whole distinction this module encodes, and the two halves below are not
// interchangeable. Size and Energy are `hummingbird_domain`'s own closed
// vocabularies — a value outside them is a bug — so they are `<select>`
// options, spelled with the wire's names as values and sentence-case labels.
// Context is free text in `items.context`, and CONTEXT.md says why: *"an open
// vocabulary, not a fixed enum … because the set of places a person works is
// theirs."* So `CONTEXTS` feeds a `Combobox` and constrains nothing.
//
// It did not use to. Both forms rendered a `<select>` over these six, which
// meant no surface in the app could enter a seventh — while `frontier-facets.ts`
// built its filter chips from the contexts actually present and sorted
// unrecognised ones in alphabetically, and `server/domain/src/item.rs` gave
// `@calls` as an example of a context nobody could type. The read side had
// always believed the glossary; only the write side disagreed, so this is the
// write side being corrected rather than a decision being made.
//
// `@waiting` is gone from the list. It failed Context's own test — *where or
// with what* an item can be done — and CONTEXT.md is flat that "External wait
// is the only meaning of the Blocked state", so a context by that name was the
// Blocked stage wearing a hard filter's clothes. Nothing is stranded by the
// removal: the string is still valid, items already carrying it still sync,
// still filter on the frontier, and can still be typed here. It is only no
// longer suggested.
//
// One module because two forms offer the same choices — the capture box and
// the item editor — and a context added to one copy and not the other is a
// list that quietly disagrees with itself depending on where you sort from.
// `frontier-facets.ts` reads `CONTEXTS` too, for its chip *order*, which is
// the third copy this consolidation finally removed.
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

/** The contexts the forms *suggest* and the frontier's chips order by — the
 * places this system's owner actually works. Never a constraint on what
 * `items.context` may hold: see the header. */
export const CONTEXTS = [
  "@home",
  "@computer",
  "@phone",
  "@errands",
  "@garden",
] as const;

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
