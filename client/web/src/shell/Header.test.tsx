// @vitest-environment jsdom

// The header's optional affordances. Each of Search, Refresh and the
// standing-questions toggle renders only when its callback is supplied — the
// rule stated in `Header.tsx`: the affordance appears where it would work.
//
// The standing-questions toggle is the one with two states. It is here rather
// than in `NowScreen.test.tsx` because the button is the shell's: `App.tsx`
// owns the collapsed state, the panel itself carries no control, and a toggle
// that lived inside the thing it hides would have to move when pressed.

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Header } from "./Header";

afterEach(cleanup);

describe("Header — the Recall trigger", () => {
  // The name is the point of the case, not incidental to it: all three
  // triggers — this button, `NavRail`'s magnifier and the phone More sheet's
  // entry — say "Search everything", and only a test that spells it out
  // catches the header drifting back to a shorter one.
  it("opens Recall from a trigger named 'Search everything'", () => {
    const onSearch = vi.fn();
    render(<Header title="What's next" onCapture={() => {}} onSearch={onSearch} />);

    fireEvent.click(screen.getByRole("button", { name: "Search everything" }));
    expect(onSearch).toHaveBeenCalledTimes(1);
  });

  it("renders no trigger when onSearch is absent", () => {
    render(<Header title="What's next" onCapture={() => {}} />);

    expect(screen.queryByRole("button", { name: "Search everything" })).toBeNull();
  });
});

describe("Header — the standing-questions toggle", () => {
  it("renders nothing when no toggle is supplied — every screen but Now", () => {
    render(<Header title="Triage" onCapture={() => {}} />);

    expect(screen.queryByRole("button", { name: "Standing questions" })).toBeNull();
  });

  it("reads as expanded while the panel is open, and calls back", () => {
    const onToggleQuestions = vi.fn();
    render(
      <Header title="What's next" onCapture={() => {}} onToggleQuestions={onToggleQuestions} />,
    );

    const toggle = screen.getByRole("button", { expanded: true, name: "Standing questions" });
    fireEvent.click(toggle);
    expect(onToggleQuestions).toHaveBeenCalledTimes(1);
  });

  it("reads as collapsed while the panel is shut, from the same one slot", () => {
    const onToggleQuestions = vi.fn();
    render(
      <Header
        title="What's next"
        onCapture={() => {}}
        onToggleQuestions={onToggleQuestions}
        questionsCollapsed
      />,
    );

    // Same accessible name in both directions: the name is what the button
    // reaches, and only `aria-expanded` says which way it currently goes.
    const toggle = screen.getByRole("button", { expanded: false, name: "Standing questions" });
    fireEvent.click(toggle);
    expect(onToggleQuestions).toHaveBeenCalledTimes(1);
  });

  it("sits between Refresh and New, so shutting the panel never moves it", () => {
    render(
      <Header
        title="What's next"
        onCapture={() => {}}
        onRefresh={() => {}}
        onToggleQuestions={() => {}}
      />,
    );

    const names = screen
      .getAllByRole("button")
      .map((button) => button.getAttribute("aria-label") ?? button.textContent);
    expect(names).toEqual(["Refresh", "Standing questions", "New"]);
  });
});
