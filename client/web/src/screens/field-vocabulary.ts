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
// **The canonical copy moved to Rust at M1-2 (ADR-0025, #141/#500):**
// `hummingbird_core::decisions::vocabulary` now owns the size/energy option
// values and the suggested `CONTEXTS` list, reusing `hummingbird_domain::
// {Size, Energy}::ALL` rather than re-deriving them, and `field-vocabulary
// .test.ts` is ported to Rust there as the canonical suite.
//
// **The arrays below stay literal TS, not a live call through the seam.**
// `SIZE_OPTIONS`/`ENERGY_OPTIONS`/`CONTEXTS` are read directly as values at
// React-render time by `ItemPanel.tsx` and `CaptureBox.tsx` — but they are
// *exported as plain constants*, and a plain `export const` computed by
// calling into wasm runs at MODULE EVALUATION, which for every file
// statically reachable from `main.tsx`'s `import { App }` happens before
// `initDecisions()` is ever awaited (`main.tsx`'s own top-level imports
// resolve before its first `await` runs). A `const` built that way would
// throw the seam's "used before ready" guard on every page load. So this
// module keeps hand-written arrays, now pinned against
// `hummingbird_core::decisions::vocabulary`'s real, seam-exposed functions
// (`sizeOptionsFromCore`/`energyOptionsFromCore`/`contextsFromCore` in
// `decisions/seam.ts`) by `field-vocabulary.test.ts`'s own pinning cases —
// the same "held together by a test" mechanism the header below used to
// warn about, except the other side of the test is now Rust rather than a
// second hand-typed TS array. `urgency.ts`/`deadline-parts.ts`/
// `capture-meta.ts`'s decision half sink fully because every one of *their*
// exports is a function, called from event handlers and render bodies —
// never at module-evaluation time — so the same seam call there is safe.
//
// It did not use to be pinned at all. Both forms rendered a `<select>` over
// these six, which meant no surface in the app could enter a seventh — while
// `frontier-facets.ts` built its filter chips from the contexts actually
// present and sorted unrecognised ones alphabetically, and
// `server/domain/src/item.rs` gave `@calls` as an example of a context nobody
// could type. The read side had always believed the glossary; only the write
// side disagreed, so this is the write side being corrected rather than a
// decision being made.
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
// `frontier-facets.ts` reads `CONTEXTS` too, for its chip *order*.
//
// **Not the capture box's sliders.** Those are indexed by *position* rather
// than by value — `capture-meta.ts`'s `CAPTURE_SIZE_NAMES`/
// `CAPTURE_ENERGY_NAMES` own that correspondence, and its test pins it. A
// slider stop index is a rendering concern (ADR-0025's verdict table:
// "capture-meta's form-adapter half … slider indices"), so it stays a
// second, deliberately TS-only array rather than routing through the seam
// — see that module's own header for the full argument.

import { contextsOf, NO_CONTEXT } from "../decisions/seam";
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

/** What a capture form should *offer* for context: the suggested `CONTEXTS`
 * first, then every other context the given live items actually carry.
 *
 * The whole point is that a context typed once is a place this person works,
 * and the next capture should offer it rather than making them retype it —
 * `CONTEXTS` alone can never grow, so a `@calls` minted yesterday was invisible
 * to the form that could have reused it. The ordering rule is not decided
 * here: `contextsOf` is `hummingbird_core::decisions::frontier::contexts_of`
 * through the seam (suggested first, extras alphabetically), and this is a
 * composition of two canonical values — pure TS under ADR-0025.
 *
 * `NO_CONTEXT` is dropped: it is `contexts_of`'s label for the *absence* of a
 * context, a facet chip's value and never a string `items.context` may hold.
 *
 * **A function, not a `const`** — deliberately, and the header above is the
 * argument: a `const` computed by calling the seam runs at module evaluation,
 * before `initDecisions()` is awaited, and would throw the "used before ready"
 * guard on every page load. Called from a render body it is safe.
 *
 * Freshness follows sync: a context typed on another device is offered here
 * once the next sync cycle has landed the item, not before. */
export function contextSuggestions(items: readonly TaskItemDTO[]): string[] {
  const seen = new Set<string>(CONTEXTS);
  const out: string[] = [...CONTEXTS];
  for (const context of contextsOf(items)) {
    if (context === NO_CONTEXT || seen.has(context)) {
      continue;
    }
    seen.add(context);
    out.push(context);
  }
  return out;
}

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
