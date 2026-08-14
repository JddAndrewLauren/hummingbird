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
import { CAPTURE_ENERGY_NAMES, CAPTURE_SIZE_NAMES } from "./capture-meta";
import { CONTEXT_OPTIONS, CONTEXTS, ENERGY_OPTIONS, SIZE_OPTIONS } from "./field-vocabulary";

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

  it("leads every list with the resting 'Not set', which maps to no value", () => {
    // Unset is a legitimate resting state on all three (deciding is mint-time
    // work), and every form relies on the empty string being first.
    for (const options of [SIZE_OPTIONS, ENERGY_OPTIONS, CONTEXT_OPTIONS]) {
      expect(options[0]).toEqual({ value: "", label: "Not set" });
      expect(options.slice(1).some((option) => option.value === "")).toBe(false);
    }
  });

  it("offers every context and invents none", () => {
    expect(CONTEXT_OPTIONS.slice(1).map((option) => option.value)).toEqual([...CONTEXTS]);
  });
});
