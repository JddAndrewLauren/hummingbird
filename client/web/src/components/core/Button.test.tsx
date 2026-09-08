// @vitest-environment jsdom

// `Button` grew an `href` when the item panel's link affordance moved into
// the attachment row and briefly stopped being a link at all — a `<button>`
// whose `onClick` called `window.open`. That looks identical and is not: an
// anchor is announced as a link, appears in a screen reader's link rotor, and
// carries middle-click, modifier-click, "Copy link address" and the
// status-bar preview, none of which survive the swap. The regression was
// invisible to every assertion that looked for the label, which is why the
// element itself is asserted here.

import { describe, expect, it, vi } from "vitest";
import { Button } from "./Button";
import { fireEvent, render, screen } from "../../test/component";

describe("Button's href arm", () => {
  it("renders an anchor, and pairs a new tab with the rel that makes it safe", () => {
    render(
      <Button href="https://example.test/x" target="_blank" variant="secondary" size="sm">
        example.test
      </Button>,
    );

    const anchor = screen.getByRole("link", { name: "example.test" });
    expect(anchor.tagName).toBe("A");
    expect(anchor.getAttribute("href")).toBe("https://example.test/x");
    expect(anchor.getAttribute("rel")).toBe("noopener noreferrer");
    // `type` belongs to a button and is meaningless — and invalid — on an
    // anchor, so the default must not leak onto it.
    expect(anchor.hasAttribute("type")).toBe(false);
  });

  it("leaves rel alone when the caller names one, and adds none for a same-tab link", () => {
    render(
      <Button href="/somewhere" target="_blank" rel="noreferrer">
        named
      </Button>,
    );
    expect(screen.getByRole("link", { name: "named" }).getAttribute("rel")).toBe("noreferrer");

    render(<Button href="/elsewhere">same tab</Button>);
    expect(screen.getByRole("link", { name: "same tab" }).hasAttribute("rel")).toBe(false);
  });

  it("still renders the icons and the label", () => {
    render(
      <Button href="https://example.test/x" iconLeft="link" iconRight="arrow-up-right">
        example.test
      </Button>,
    );
    expect(screen.getByRole("link", { name: "example.test" }).querySelectorAll("svg").length).toBe(2);
  });

  /** An anchor has no disabled state, so `href` with `disabled`/`loading` is
   * the caller contradicting itself. It falls back to the button, which DOES
   * have one, rather than rendering a link that looks inert and is not. */
  it("falls back to a real button when disabled or loading", () => {
    render(
      <Button href="https://example.test/x" disabled>
        off
      </Button>,
    );
    expect(screen.queryByRole("link", { name: "off" })).toBeNull();
    expect((screen.getByRole("button", { name: "off" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("without an href it is still a button, and still calls onClick", () => {
    const onClick = vi.fn();
    render(<Button onClick={onClick}>Act</Button>);

    const button = screen.getByRole("button", { name: "Act" });
    expect(button.tagName).toBe("BUTTON");
    fireEvent.click(button);
    expect(onClick).toHaveBeenCalledTimes(1);
  });
});
