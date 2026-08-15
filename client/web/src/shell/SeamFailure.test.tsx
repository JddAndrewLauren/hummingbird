// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "../test/component";
import { SeamFailure } from "./SeamFailure";

describe("SeamFailure", () => {
  it("says what failed without reassuring anyone", () => {
    render(<SeamFailure detail="wasm compile refused by CSP" onReload={vi.fn()} />);
    expect(screen.getByText("hummingbird can't start")).toBeTruthy();
    // The cause is shown verbatim: it is the only clue a reader has, and
    // this surface exists because the store's copy of it gets overwritten.
    expect(screen.getByText("wasm compile refused by CSP")).toBeTruthy();
  });

  it("offers the one control that can help", () => {
    const onReload = vi.fn();
    render(<SeamFailure detail="boom" onReload={onReload} />);
    fireEvent.click(screen.getByText("Reload"));
    expect(onReload).toHaveBeenCalledTimes(1);
  });

  it("is announced, not just drawn", () => {
    const { container } = render(<SeamFailure detail="boom" onReload={vi.fn()} />);
    expect(container.querySelector("[role='alert']")).toBeTruthy();
  });
});
