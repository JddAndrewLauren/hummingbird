// @vitest-environment jsdom

// The collapsed rail's contract: icons and counts only, with nothing losing
// its name. A pure module can't see whether the label really left the DOM or
// whether the count survived the collapse — only a mount can (the same
// "typecheck cannot see that something has no reader" reasoning every other
// component test here records).

import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "../test/component";
import { APP_VERSION } from "./build-version";
import type { Band as PaneBand } from "../screens/questions/contract";
import { NavRail } from "./NavRail";

function renderRail(
  collapsed: boolean,
  onSearch?: () => void,
  statusAlarm?: PaneBand,
  screenName: "now" | "status" = "now",
) {
  const onScreen = vi.fn();
  const onToggleCollapsed = vi.fn();
  const onHome = vi.fn();
  render(
    <NavRail
      screen={screenName}
      onScreen={onScreen}
      counts={{ triage: 4, alerts: 3 }}
      statusLabel="api v1 · core ready"
      statusAlarm={statusAlarm}
      theme="light"
      onToggleTheme={() => {}}
      collapsed={collapsed}
      onToggleCollapsed={onToggleCollapsed}
      onHome={onHome}
      onSearch={onSearch}
    />,
  );
  return { onScreen, onToggleCollapsed, onHome };
}

describe("NavRail — collapsed", () => {
  it("drops the labels, wordmark and status line but keeps every button named, with its count", () => {
    renderRail(true);

    expect(screen.queryByText("Triage")).toBeNull();
    expect(screen.queryByText("hummingbird")).toBeNull();
    expect(screen.queryByText("api v1 · core ready")).toBeNull();
    expect(screen.queryByText(`v${APP_VERSION}`)).toBeNull();

    // Named via aria-label, still navigating.
    const triage = screen.getByRole("button", { name: "Triage" });
    expect(triage).toBeDefined();
    // The counts are the part a collapsed rail must not lose.
    expect(screen.getByText("4")).toBeDefined();
    expect(screen.getByText("3")).toBeDefined();

    expect(screen.getByRole("button", { name: "Expand the sidebar" })).toBeDefined();
  });

  it("expanded shows labels, status line and the collapse control", () => {
    const { onToggleCollapsed } = renderRail(false);

    expect(screen.getByText("Triage")).toBeDefined();
    expect(screen.getByText("hummingbird")).toBeDefined();
    expect(screen.getByText("api v1 · core ready")).toBeDefined();
    // The build version is a module constant, not a prop — this is the only
    // gate that it actually reaches the footer.
    expect(screen.getByText(`v${APP_VERSION}`)).toBeDefined();

    fireEvent.click(screen.getByRole("button", { name: "Collapse the sidebar" }));
    expect(onToggleCollapsed).toHaveBeenCalledTimes(1);
  });

  it("still navigates while collapsed", () => {
    const { onScreen } = renderRail(true);
    fireEvent.click(screen.getByRole("button", { name: "Ledger" }));
    expect(onScreen).toHaveBeenCalledWith("ledger");
  });

  // The mark is the way home in both forms — collapsed it is the only thing
  // left of the wordmark, so it must stay a control.
  it.each([true, false])("the mark goes home, collapsed=%s", (collapsed) => {
    const { onHome } = renderRail(collapsed);
    fireEvent.click(screen.getByRole("button", { name: "hummingbird — go to Now and refresh" }));
    expect(onHome).toHaveBeenCalledTimes(1);
  });

  // #480: the magnifier is `onSearch`'s own affordance — absent when the
  // caller has no search to open, same rule as `Header`'s. `App.tsx` always
  // has one to pass; this exercises the prop's own contract, not a real path.
  it("opens Recall from the magnifier when onSearch is supplied", () => {
    const onSearch = vi.fn();
    renderRail(false, onSearch);
    fireEvent.click(screen.getByRole("button", { name: "Search everything" }));
    expect(onSearch).toHaveBeenCalledTimes(1);
  });

  it("renders no magnifier when onSearch is absent", () => {
    renderRail(false);
    expect(screen.queryByRole("button", { name: "Search everything" })).toBeNull();
  });
});

// The Status control's tint (`nav-alarm.ts` decides the colour; the core
// decides the band). A pure module cannot see which element the colour
// actually landed on, which is the whole reason this is a mount.
describe("NavRail — the Status alarm", () => {
  it("leaves every control its ordinary colour when nothing raises the nav", () => {
    renderRail(false);
    expect(screen.getByRole("button", { name: "Status" }).style.color).toBe(
      "var(--text-secondary)",
    );
  });

  it("tints Status, and only Status, with the band's colour", () => {
    renderRail(false, undefined, "near");
    expect(screen.getByRole("button", { name: "Status" }).style.color).toBe(
      "var(--status-warn-fg)",
    );
    // The alarm belongs to one destination — a tint that leaked would read
    // as "everything is wrong".
    expect(screen.getByRole("button", { name: "Triage" }).style.color).toBe(
      "var(--text-secondary)",
    );
  });

  it("keeps the tint while Status is the open screen — the rail says 'you are here' with its own fill", () => {
    renderRail(false, undefined, "live", "status");
    const status = screen.getByRole("button", { name: "Status" });
    expect(status.style.color).toBe("var(--status-danger-fg)");
    expect(status.getAttribute("aria-current")).toBe("page");
    expect(status.style.background).toBe("var(--accent-quiet)");
  });
});
