// The assertion that was missing when ADR-0024 renamed the middle size.
//
// `short` -> `normal` reached `capture-meta.ts`, `size-energy.ts`, the wire
// types and the server. It did not reach `SIZE_OPTIONS` in this module, which
// was typed `{ value: string }` at the time and so compiled perfectly with a
// value the vocabulary no longer had. The server accepts `short` as a serde
// alias, so nothing failed: the item editor went on writing successfully and
// only the word on screen was wrong. A green typecheck, a green suite and a
// screenshot all missed it.
//
// Two guards now stand where none did. `LevelOption<T>` anchors the option
// values to `TaskItemDTO`'s own union, so the next rename is a build error —
// that is the real fix, and it is in the module itself. These tests are the
// second: they pin the two *directions* a type cannot check, that this module
// and `capture-meta.ts` describe the same set in the same order, and that the
// resting "not set" entry is where the forms expect it.

import { describe, expect, it } from "vitest";
import {
  contextsFromCore,
  energyOptionsFromCore,
  NO_CONTEXT,
  sizeOptionsFromCore,
} from "../decisions/seam";
import type { TaskItemDTO } from "../store/protocol";
import { CAPTURE_ENERGY_NAMES, CAPTURE_SIZE_NAMES } from "./capture-meta";
import { CONTEXTS, contextSuggestions, ENERGY_OPTIONS, SIZE_OPTIONS } from "./field-vocabulary";

/** A minimal live item — only `context` is ever read here. Spelled out
 * locally rather than imported from `test/component.tsx`: that helper pulls
 * in testing-library, and this file needs no DOM. */
function item(context: string | null): TaskItemDTO {
  return {
    id: `id-${context ?? "none"}`,
    seq: null,
    title: "untitled",
    description: null,
    stage: "ready",
    size: null,
    energy: null,
    context,
    priority: 0,
    projectId: null,
    projectPos: null,
    deadline: null,
    scheduledDate: null,
    source: null,
    sourceKey: null,
    sourceUrl: null,
    vaultPath: null,
    archivedAt: null,
    createdAt: 1,
    updatedAt: 1,
    version: 0,
    pending: false,
  };
}

describe("field-vocabulary — the size and energy option lists", () => {
  it("offers exactly the wire's size names, in the slider's own order", () => {
    // `slice(1)` drops the resting entry, which has no wire name by design.
    expect(SIZE_OPTIONS.slice(1).map((option) => option.value)).toEqual([
      ...CAPTURE_SIZE_NAMES,
    ]);
  });

  it("offers exactly the wire's energy names, in the slider's own order", () => {
    expect(ENERGY_OPTIONS.slice(1).map((option) => option.value)).toEqual([
      ...CAPTURE_ENERGY_NAMES,
    ]);
  });

  it("names the middle size `normal`, which is the value ADR-0024 renamed", () => {
    // Spelled out rather than left to the comparison above, because both
    // sides of that test could drift together. This is the literal the
    // server's own `Size` vocabulary contains — `short` survives there only
    // as a serde alias for reading old rows, and must never be written.
    expect(SIZE_OPTIONS.map((option) => option.value)).toEqual([
      "",
      "quick",
      "normal",
      "deep",
    ]);
    expect(SIZE_OPTIONS.map((option) => option.value)).not.toContain("short");
  });

  it("leads both closed lists with the resting 'Not set', which maps to no value", () => {
    // Unset is a legitimate resting state (deciding is mint-time work), and
    // both forms rely on the empty string being first. Context has no such
    // entry to check: it is a `Combobox`, where rest is an empty field.
    for (const options of [SIZE_OPTIONS, ENERGY_OPTIONS]) {
      expect(options[0]).toEqual({ value: "", label: "Not set" });
      expect(options.slice(1).some((option) => option.value === "")).toBe(false);
    }
  });
});

describe("field-vocabulary — the context suggestions", () => {
  it("suggests the places this system's owner works", () => {
    expect([...CONTEXTS]).toEqual([
      "@home",
      "@computer",
      "@phone",
      "@errands",
      "@garden",
      "@homework",
    ]);
  });

  it("suggests `@homework` last, so no existing chip's order moves", () => {
    // `frontier-facets.ts` reads `CONTEXTS` for its chip order. #675's entry
    // is appended for that reason alone — and it is a topic rather than a
    // place, the exception CONTEXT.md's Context entry now records.
    expect(CONTEXTS[CONTEXTS.length - 1]).toBe("@homework");
  });

  it("does not suggest `@waiting`, which was the Blocked stage in disguise", () => {
    // CONTEXT.md: "External wait is the only meaning of the Blocked state."
    // A context named for waiting is a second home for that idea, and it fails
    // Context's own test — *where or with what* the work can be done.
    expect([...CONTEXTS]).not.toContain("@waiting");
  });

  it("carries no resting entry, because an empty context is not a choice in a list", () => {
    // The `<select>` needed a `""` option to express "not set". A text field
    // expresses it by being empty, so a `""` in here would be an offered
    // suggestion of nothing.
    expect([...CONTEXTS]).not.toContain("");
  });
});

// The composition the capture form actually offers. The ordering rule is
// Rust's (`contexts_of`) and pinned by `frontier-facets.test.ts`; what is
// provable here is the union itself — that the suggested list is never lost,
// that a context somebody typed reaches the form that could reuse it, and
// that neither duplicates nor `NO_CONTEXT` leak into a list of places.
describe("field-vocabulary — contextSuggestions", () => {
  it("offers exactly the suggested list when no item carries a context", () => {
    expect(contextSuggestions([])).toEqual([...CONTEXTS]);
    expect(contextSuggestions([item(null)])).toEqual([...CONTEXTS]);
  });

  it("appends a context the suggested list has never heard of", () => {
    // The paper cut itself: `@calls` was mintable and then invisible to the
    // next capture, because `CONTEXTS` cannot grow.
    expect(contextSuggestions([item("@calls")])).toEqual([...CONTEXTS, "@calls"]);
  });

  it("never offers the same place twice", () => {
    const offered = contextSuggestions([item("@home"), item("@home")]);
    expect(offered).toEqual([...CONTEXTS]);
    expect(new Set(offered).size).toBe(offered.length);
  });

  it("drops NO_CONTEXT, which names an absence rather than a place", () => {
    // `contexts_of` ends with it whenever anything is unfiled, and every
    // capture with no context set is exactly that — so this is the common
    // case, not an edge one.
    expect(contextSuggestions([item(null), item("@calls")])).not.toContain(NO_CONTEXT);
  });

  it("keeps the extras in the core's order, which is alphabetical", () => {
    expect(contextSuggestions([item("@zeta"), item("@alpha")])).toEqual([
      ...CONTEXTS,
      "@alpha",
      "@zeta",
    ]);
  });
});

// M1-2 (#500): the canonical copy of this vocabulary moved to
// `hummingbird_core::decisions::vocabulary`. The literal arrays above stay
// TS (`field-vocabulary.ts`'s own header says why — a module-evaluation-
// order constraint), so these are the pinning cases that keep them provably
// equal to the Rust source rather than merely a second hand-typed list next
// to it: this is the exact assertion that was missing when ADR-0024 renamed
// the middle size, now checked against the crate that cannot drift from
// `hummingbird_domain::Size`/`Energy` because it is built on them.
describe("field-vocabulary — pinned against hummingbird_core::decisions::vocabulary", () => {
  it("SIZE_OPTIONS' real values match the core's size vocabulary", () => {
    expect(SIZE_OPTIONS.slice(1)).toEqual(sizeOptionsFromCore());
  });

  it("ENERGY_OPTIONS' real values match the core's energy vocabulary", () => {
    expect(ENERGY_OPTIONS.slice(1)).toEqual(energyOptionsFromCore());
  });

  it("CONTEXTS matches the core's suggested context list", () => {
    expect([...CONTEXTS]).toEqual(contextsFromCore());
  });
});
