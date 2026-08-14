// @vitest-environment jsdom

// The phone nav's contract. `nav-bar.test.ts` proves the partition is total;
// this proves the two halves are actually reachable and that the build version
// has somewhere to live — the whole reason the More sheet exists rather than
// four screens and a shrug.
//
// `NavBar` is rendered directly, never through `useIsPhone`. jsdom's
// `matchMedia` always reports `matches: false`, so a test that drove `App` and
// forgot to stub it would render the desktop rail and pass its assertions
// against that — the silent failure `useIsPhone`'s header warns about.

import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen } from "../test/component";
import { APP_VERSION } from "./build-version";
import { NavBar } from "./NavBar";
import { NAV_BAR_OVERFLOW, NAV_BAR_PRIMARY } from "./nav-bar";
import { SCREEN_LABELS, type Screen } from "./screens";

// The sheet is controlled by the shell now (`escape-claimants.ts`), so these
// tests supply the state `App` supplies. Escape is deliberately not exercised
// here any more: `NavBar` binds no key handler at all, and which overlay owns
// an Escape is `escape-claimants.test.ts`'s subject.
function renderBar(screenName: Screen = "now") {
  const onScreen = vi.fn();
  const onToggleTheme = vi.fn();
  let setSheetOpen!: (open: boolean) => void;

  function Harness() {
    const [sheetOpen, setOpen] = useState(false);
    setSheetOpen = setOpen;
    return (
      <NavBar
        screen={screenName}
        onScreen={onScreen}
        counts={{ triage: 4, alerts: 3 }}
        statusLabel="api v1 · core ready"
        theme="light"
        onToggleTheme={onToggleTheme}
        sheetOpen={sheetOpen}
        onSheetOpen={setOpen}
      />
    );
  }

  render(<Harness />);
  /** The shell closing the sheet from outside — what Escape now does. */
  const closeFromShell = () => act(() => setSheetOpen(false));
  return { onScreen, onToggleTheme, closeFromShell };
}

function openMore() {
  fireEvent.click(screen.getByRole("button", { name: /^More$/ }));
}

describe("NavBar", () => {
  it("puts the four primary screens on the bar, with their counts", () => {
    renderBar();
    for (const item of NAV_BAR_PRIMARY) {
      expect(screen.getByRole("button", { name: SCREEN_LABELS[item] })).toBeDefined();
    }
    expect(screen.getByText("4")).toBeDefined();
    expect(screen.getByText("3")).toBeDefined();
  });

  it("keeps the overflow screens out of the bar until More is opened", () => {
    renderBar();
    for (const item of NAV_BAR_OVERFLOW) {
      expect(screen.queryByRole("button", { name: SCREEN_LABELS[item] })).toBeNull();
    }
    openMore();
    for (const item of NAV_BAR_OVERFLOW) {
      expect(screen.getByRole("button", { name: SCREEN_LABELS[item] })).toBeDefined();
    }
  });

  // The bug this whole change set started from: an operator's phone showing an
  // old build with no version anywhere on screen. On a phone the rail footer
  // does not exist, so this sheet and Settings are the only two places it
  // renders — and without this assertion a later refactor drops it silently.
  it("the build version is reachable on the phone form", () => {
    renderBar();
    expect(screen.queryByText(`v${APP_VERSION}`)).toBeNull();
    openMore();
    expect(screen.getByText(`v${APP_VERSION}`)).toBeDefined();
    expect(screen.getByText("api v1 · core ready")).toBeDefined();
  });

  // The theme toggle lives in the rail's footer on a desktop and has nowhere
  // else to go here.
  it("the theme toggle moves into the sheet", () => {
    const { onToggleTheme } = renderBar();
    openMore();
    fireEvent.click(screen.getByRole("button", { name: "Switch to dark theme" }));
    expect(onToggleTheme).toHaveBeenCalledTimes(1);
  });

  it("navigating from the bar and from the sheet both report the screen", () => {
    const { onScreen } = renderBar();
    fireEvent.click(screen.getByRole("button", { name: "Triage" }));
    expect(onScreen).toHaveBeenCalledWith("triage");

    openMore();
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect(onScreen).toHaveBeenCalledWith("settings");
  });

  it("choosing from the sheet closes it", () => {
    renderBar();
    openMore();
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  // The sheet closes when the shell says so and binds nothing of its own —
  // which is what stops one Escape closing this AND an item panel behind it
  // (`escape-claimants.ts`). A stray key must not reopen or hold it.
  it("shuts when the shell shuts it, and owns no key handler of its own", () => {
    const { closeFromShell } = renderBar();
    openMore();
    expect(screen.getByRole("dialog")).toBeDefined();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.getByRole("dialog")).toBeDefined();
    closeFromShell();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  // Without this the bar shows nothing selected while an overflow screen is
  // open, which reads as "you are nowhere".
  it("More reads as current while an overflow screen is open, and the primary buttons do not", () => {
    renderBar("settings");
    const more = screen.getByRole("button", { name: /^More$/ });
    expect(more.getAttribute("aria-expanded")).toBe("false");
    // Not `aria-current`: More opens a panel, it is not itself a page.
    expect(more.getAttribute("aria-current")).toBeNull();
    expect(more.style.color).toBe("var(--text-brand)");

    for (const item of NAV_BAR_PRIMARY) {
      const button = screen.getByRole("button", { name: SCREEN_LABELS[item] });
      expect(button.getAttribute("aria-current")).toBeNull();
    }
  });

  it("the open primary screen is the current page", () => {
    renderBar("alerts");
    expect(
      screen.getByRole("button", { name: "Alerts" }).getAttribute("aria-current"),
    ).toBe("page");
  });

  // Exactly one navigation landmark, in either nav form — `surfaces.spec.ts`
  // queries `getByRole("navigation")` in strict mode.
  it("is one navigation landmark, sheet open or shut", () => {
    renderBar();
    expect(screen.getAllByRole("navigation")).toHaveLength(1);
    openMore();
    expect(screen.getAllByRole("navigation")).toHaveLength(1);
  });

  // The sheet's controls are not navigation: five of them are, but the close
  // button and the theme toggle are not, and inside the landmark all seven are
  // announced as part of "Surfaces". The landmark assertion above cannot see
  // this — a dialog nested in the nav is still one landmark.
  it("the sheet is outside the navigation landmark", () => {
    renderBar();
    openMore();
    const nav = screen.getByRole("navigation");
    expect(nav.contains(screen.getByRole("dialog"))).toBe(false);
  });

  // `aria-modal="true"` hides everything outside the panel from assistive
  // tech, and the More button that opened it is outside. Focus left there is
  // focus on an element the user can no longer reach.
  it("moves focus into the sheet on open", () => {
    renderBar();
    openMore();
    expect(document.activeElement).toBe(screen.getByRole("dialog"));
  });

  // Every way out of this sheet is a way back to the bar, so all three of them
  // have to hand focus back to the control that opened it.
  it.each([
    ["the shell (Escape)", (bar: ReturnType<typeof renderBar>) => bar.closeFromShell()],
    ["the close button", () => fireEvent.click(screen.getByRole("button", { name: "Close" }))],
    ["choosing a screen", () => fireEvent.click(screen.getByRole("button", { name: "Settings" }))],
  ])("returns focus to More when the sheet is closed by %s", (_name, close) => {
    const bar = renderBar();
    const more = screen.getByRole("button", { name: /^More$/ });
    more.focus();
    openMore();
    close(bar);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(more);
  });
});
