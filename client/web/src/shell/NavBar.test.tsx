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

import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "../test/component";
import { APP_VERSION } from "./build-version";
import { NavBar } from "./NavBar";
import { NAV_BAR_OVERFLOW, NAV_BAR_PRIMARY } from "./nav-bar";
import { SCREEN_LABELS, type Screen } from "./screens";

function renderBar(screenName: Screen = "now") {
  const onScreen = vi.fn();
  const onToggleTheme = vi.fn();
  render(
    <NavBar
      screen={screenName}
      onScreen={onScreen}
      counts={{ triage: 4, alerts: 3 }}
      statusLabel="api v1 · core ready"
      theme="light"
      onToggleTheme={onToggleTheme}
    />,
  );
  return { onScreen, onToggleTheme };
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

  it("Escape closes the sheet", () => {
    renderBar();
    openMore();
    expect(screen.getByRole("dialog")).toBeDefined();
    fireEvent.keyDown(document, { key: "Escape" });
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
});
