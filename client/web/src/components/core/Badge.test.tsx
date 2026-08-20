// @vitest-environment jsdom

// `Badge` grew an icon-only form when size and energy lost their words (#446).
// The pill is a flex row with a `gap`, so an empty label span is not
// invisible — it takes the gap and the padding after it, and the glyph stops
// sitting centred. That is a layout bug no screenshot review reliably catches
// on a 22px pill, so the collapse is asserted here instead.

import { describe, expect, it } from "vitest";
import { Badge } from "./Badge";
import { render, screen } from "../../test/component";

const pill = () => screen.getByTestId("b");

describe("the label span exists only when there is a label", () => {
  it("collapses for an icon-only badge", () => {
    render(<Badge data-testid="b" icon="inbox" />);
    // One child: the glyph. Not two, the second being an empty span.
    expect(pill().children).toHaveLength(1);
  });

  it("is present when there are children", () => {
    render(<Badge data-testid="b" icon="inbox">Triage</Badge>);
    expect(pill().children).toHaveLength(2);
    expect(screen.getByText("Triage")).toBeDefined();
  });

  // The `{cond && "label"}` idiom hands React `false`, which renders nothing.
  // A truthy/falsy test would be wrong in the other direction, so both of
  // these matter: `false` must collapse and `0` must not.
  it("collapses for `false`, the shape a short-circuited label takes", () => {
    render(<Badge data-testid="b" icon="inbox">{false}</Badge>);
    expect(pill().children).toHaveLength(1);
  });

  it("keeps a zero, which is a real label and not an absence", () => {
    render(<Badge data-testid="b" icon="inbox">{0}</Badge>);
    expect(pill().children).toHaveLength(2);
    expect(screen.getByText("0")).toBeDefined();
  });
});

describe("the wrap variant (#374)", () => {
  it("defaults to the single-line pill: nowrap, fixed height, pill radius, solid leading", () => {
    render(<Badge data-testid="b">short</Badge>);
    const style = pill().style;
    expect(style.whiteSpace).toBe("nowrap");
    expect(style.height).toBe("22px");
    expect(style.borderRadius).toBe("var(--radius-pill)");
    expect(style.font).toContain("/1 ");
  });

  it("wraps sentence-length text instead of clipping, at a softer radius and body-sm's declared leading", () => {
    render(<Badge data-testid="b" wrap>Unranked severity — loses every fold against a declared severity, and can never escalate a notification.</Badge>);
    const style = pill().style;
    expect(style.whiteSpace).toBe("normal");
    expect(style.height).toBe("");
    expect(style.borderRadius).toBe("var(--radius-md)");
    // Solid (1) leading collides 2-3 wrapped lines; body-sm's own token
    // (`design/tokens/typography.css`) declares 1.45 (#374 review).
    expect(style.font).toContain("/1.45 ");
  });
});

describe("the icon-only form is nameable", () => {
  // Not `Badge`'s own doing — it spreads the caller's props onto the pill.
  // This pins that the spread reaches the element, because the word-free
  // chips' whole accessibility story depends on it.
  it("passes an aria-label through to the pill", () => {
    render(<Badge data-testid="b" icon="inbox" role="img" aria-label="Triage" />);
    expect(screen.getByRole("img", { name: "Triage" })).toBe(pill());
  });
});
