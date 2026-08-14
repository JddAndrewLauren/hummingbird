// @vitest-environment jsdom

// `StageBadge` renders one stage two ways, and which way is data — an `icon`
// on the `STAGES` row. Only `triage` has one so far (#446), so this file
// pins both halves of that branch: the glyph form has to name itself with no
// word to read, and the worded form has to be left alone. Without the second
// half, giving every stage a glyph later would look like a passing change.

import { describe, expect, it } from "vitest";
import { StageBadge } from "./StageBadge";
import { render, screen } from "../../test/component";

describe("the triage stage draws as a glyph", () => {
  it("carries no word, and names itself instead", () => {
    render(<StageBadge stage="triage" />);
    expect(screen.queryByText(/triage/i)).toBeNull();
    // Role and name, not a `title` attribute: a title on a generic span is
    // announced inconsistently, so asserting it would prove the tooltip and
    // not the accessible name that licenses dropping the word.
    expect(screen.getByRole("img", { name: "Triage" })).toBeDefined();
  });

  it("draws a glyph and keeps the stage colour", () => {
    render(<StageBadge stage="triage" />);
    const pill = screen.getByRole("img", { name: "Triage" });
    expect(pill.style.color).toBe("var(--stage-triage)");
    expect(pill.style.background).toBe("var(--stage-triage-bg)");
    // `not.toBeNull`: a failed lookup is `null`, which *is* defined.
    expect(pill.querySelector("svg")).not.toBeNull();
  });

  // The glyph replaces the dot as well as the word — the dot exists to carry
  // the stage colour where there is no other mark, and a coloured glyph
  // already does that. Two marks for one fact is the thing being prevented.
  it("does not also draw the dot", () => {
    render(<StageBadge stage="triage" />);
    // Exactly one child, and it is the glyph — a surviving dot would be a
    // second span beside it.
    const pill = screen.getByRole("img", { name: "Triage" });
    expect(pill.children).toHaveLength(1);
    expect(pill.children[0].querySelector("svg")).not.toBeNull();
  });
});

describe("every other stage keeps its dot and its word", () => {
  it("still spells the stage out", () => {
    render(<StageBadge stage="blocked" />);
    expect(screen.getByText("Blocked")).toBeDefined();
    expect(screen.queryByRole("img")).toBeNull();
  });

  it("keeps its own colour, so the glyph branch changed nothing here", () => {
    render(<StageBadge stage="ready" />);
    const pill = screen.getByText("Ready");
    expect(pill.style.color).toBe("var(--stage-ready)");
  });
});

// The dense-row form, untouched by the glyph work: an 8px dot that has always
// carried its stage name as a tooltip.
describe("compact", () => {
  it("is a bare dot with the stage as its title", () => {
    render(<StageBadge stage="triage" compact />);
    expect(screen.getByTitle("Triage")).toBeDefined();
    expect(screen.queryByRole("img")).toBeNull();
  });
});
