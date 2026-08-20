import { describe, expect, it } from "vitest";
import { visibleSuggestions } from "./combobox-options";

const CONTEXTS = ["@home", "@computer", "@phone", "@errands", "@garden", "@shopping"];

describe("visibleSuggestions", () => {
  // The bug, stated as a test: #641 leaves a context sticky in the box, and
  // the reader asking to see the list means the whole list. The native
  // `<datalist>` could not be told this, which is why it is gone.
  it("browses the whole list whatever the box holds", () => {
    expect(visibleSuggestions(CONTEXTS, "@errands", true)).toEqual(CONTEXTS);
  });

  it("filters on a substring, case-insensitively and anywhere in the word", () => {
    expect(visibleSuggestions(CONTEXTS, "HOM", false)).toEqual(["@home"]);
    expect(visibleSuggestions(CONTEXTS, "n", false)).toEqual([
      "@phone",
      "@errands",
      "@garden",
      "@shopping",
    ]);
  });

  it("keeps the caller's order", () => {
    expect(visibleSuggestions(CONTEXTS, "@", false)).toEqual(CONTEXTS);
  });

  it("shows everything for an empty query in either mode", () => {
    expect(visibleSuggestions(CONTEXTS, "", false)).toEqual(CONTEXTS);
    expect(visibleSuggestions(CONTEXTS, "   ", false)).toEqual(CONTEXTS);
    expect(visibleSuggestions(CONTEXTS, "", true)).toEqual(CONTEXTS);
  });

  it("matches nothing when nothing matches", () => {
    expect(visibleSuggestions(CONTEXTS, "@boat", false)).toEqual([]);
  });

  it("copies rather than aliasing the caller's array", () => {
    const browsed = visibleSuggestions(CONTEXTS, "", true);
    browsed.push("@boat");
    expect(CONTEXTS).toHaveLength(6);
  });
});
