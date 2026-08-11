// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "../test/component";
import { UpdateBanner } from "./UpdateBanner";

describe("UpdateBanner", () => {
  it("says a new version is ready, politely", () => {
    render(<UpdateBanner onReload={vi.fn()} />);
    expect(
      screen.getByText("A new version of hummingbird is ready. Reloading updates every open tab."),
    ).toBeTruthy();
    expect(screen.getByRole("status").getAttribute("aria-live")).toBe("polite");
  });

  // Pinned as its own assertion because the scope sentence is the whole of
  // what makes an origin-wide reload announced rather than merely consented
  // to in one window — see the component's own header. Dropping it as
  // wordiness would silently reopen that.
  it("says the reload reaches every open tab", () => {
    render(<UpdateBanner onReload={vi.fn()} />);
    expect(screen.getByRole("status").textContent).toContain("every open tab");
  });

  it("applies the waiting worker when Reload is clicked", () => {
    const onReload = vi.fn();
    render(<UpdateBanner onReload={onReload} />);
    fireEvent.click(screen.getByRole("button", { name: "Reload" }));
    expect(onReload).toHaveBeenCalledTimes(1);
  });
});
