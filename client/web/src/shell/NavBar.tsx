import { useEffect, useRef, useState } from "react";
import { Badge } from "../components/core/Badge";
import { Icon, type IconName } from "../components/core/Icon";
import { IconButton } from "../components/core/IconButton";
import type { ResolvedTheme } from "../theme/theme";
import { NAV_BAR_OVERFLOW, NAV_BAR_PRIMARY, isOverflowScreen } from "./nav-bar";
import { SCREEN_ICONS } from "./screen-icons";
import { SCREEN_LABELS, type Screen } from "./screens";
import { ShellMeta } from "./ShellMeta";

export interface NavBarProps {
  screen: Screen;
  onScreen: (screen: Screen) => void;
  /** Per-screen counts. Absent counts render no badge — a zero pill would be
   * decoration, and an invented one would be a lie (`NavRail`'s rule). */
  counts?: Partial<Record<Screen, number>>;
  /** The core's state, already formatted (see `status-label.ts`). */
  statusLabel: string;
  theme: ResolvedTheme;
  onToggleTheme: () => void;
}

/** The phone form of the nav rail: four screens on a bar, the rest behind a
 * More sheet (`nav-bar.ts` owns which four and why).
 *
 * **In the flow, not fixed.** It is a flex sibling of `<main>` inside
 * `.hb-shell`, which is `height: 100dvh; overflow: hidden` — so the bar is
 * always on screen without `position: fixed`, and the scroll container above
 * it needs no bar-shaped padding at its foot. Two things fall out of that:
 * a screenshot of the viewport is a screenshot of the whole app (a fixed bar
 * photographs at the bottom of the *document* instead), and there is no
 * measured bar height for CSS to keep in step with.
 *
 * `App` mounts exactly one of this and `NavRail` — never both, even hidden.
 * Two navigation landmarks break `surfaces.spec.ts`'s strict-mode
 * `getByRole("navigation")` across all four visual projects.
 *
 * **The sheet is a sibling of the `<nav>`, not a child of it.** Inside the
 * landmark, its five destinations, its close button and the theme toggle are
 * all announced as part of "Surfaces" — so a screen-reader user walking the
 * navigation hears a control set that is not on the bar and mostly is not
 * navigation at all. Hoisting it out of the landmark costs nothing: it is
 * `position: fixed`, so it is out of flow wherever it sits and the flex layout
 * of `.hb-shell` never sees it, and a `dialog` is not a landmark, so the one-
 * landmark invariant above still holds with the sheet open.
 *
 * The rail's collapse toggle has no counterpart here and `rail-collapse.ts` is
 * simply unread on a phone. Coupling that stored preference to the media query
 * would make the desktop toggle behave unpredictably after a window resize. */
export function NavBar({
  screen,
  onScreen,
  counts = {},
  statusLabel,
  theme,
  onToggleTheme,
}: NavBarProps) {
  const [sheetOpen, setSheetOpen] = useState(false);
  const overflowActive = isOverflowScreen(screen);

  // Escape closes the sheet — bound to the document, not the sheet's markup,
  // for `CapturePopover`'s reason: an Escape must still close this if focus
  // has left the panel, and a handler on the markup only sees what bubbles
  // out of it.
  useEffect(() => {
    if (!sheetOpen) {
      return;
    }
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        setSheetOpen(false);
      }
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [sheetOpen]);

  function go(next: Screen) {
    onScreen(next);
    setSheetOpen(false);
  }

  return (
    <>
      <nav
        aria-label="Surfaces"
        style={{
          flex: "0 0 auto",
          display: "flex",
          alignItems: "stretch",
          background: "var(--surface-quiet)",
          borderTop: "1px solid var(--border-subtle)",
          // The home indicator's strip. Resolves to 0 anywhere without one, and
          // needs `viewport-fit=cover` in index.html to resolve to anything at
          // all on iOS.
          paddingBottom: "env(safe-area-inset-bottom)",
        }}
      >
        {NAV_BAR_PRIMARY.map((item) => (
          <NavBarButton
            key={item}
            label={SCREEN_LABELS[item]}
            icon={SCREEN_ICONS[item]}
            active={item === screen}
            count={counts[item]}
            danger={item === "alerts"}
            onClick={() => go(item)}
          />
        ))}
        {/* More reads as the current location whenever the open screen is one
            it hides — otherwise the bar shows nothing selected while Settings
            is open, which reads as "you are nowhere". */}
        <NavBarButton
          label="More"
          icon="ellipsis"
          active={overflowActive}
          expanded={sheetOpen}
          onClick={() => setSheetOpen((open) => !open)}
        />
      </nav>

      {sheetOpen ? (
        <MoreSheet
          screen={screen}
          counts={counts}
          statusLabel={statusLabel}
          theme={theme}
          onToggleTheme={onToggleTheme}
          onScreen={go}
          onClose={() => setSheetOpen(false)}
        />
      ) : null}
    </>
  );
}

function NavBarButton({
  label,
  icon,
  active,
  count,
  danger = false,
  expanded,
  onClick,
}: {
  label: string;
  icon: IconName;
  active: boolean;
  count?: number;
  danger?: boolean;
  /** Set only on the More control — it opens a panel rather than navigating,
   * so it carries `aria-expanded` instead of being a plain destination. */
  expanded?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      // Named explicitly, `NavRail`'s rule: without it the count badge inside
      // the button joins the computed name and Triage announces itself as
      // "Triage 4" — a name that changes every time the inbox does.
      aria-label={label}
      // `aria-current="page"` is how the rail says it too (`NavRail`), so both
      // nav forms announce the current surface the same way. The More control
      // is not a page, so it takes `aria-expanded` and no `aria-current`.
      aria-current={expanded === undefined && active ? "page" : undefined}
      aria-expanded={expanded}
      style={{
        flex: 1,
        // --touch-min. Four screens plus More is five targets in 390px, which
        // is 78px each — wide enough; the height is what has to be defended.
        minHeight: "var(--touch-min)",
        minWidth: "var(--touch-min)",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: "var(--space-1)",
        position: "relative",
        padding: "var(--space-4) var(--space-2)",
        background: "transparent",
        color: active ? "var(--text-brand)" : "var(--text-secondary)",
        border: "none",
        cursor: "pointer",
        transition: "color var(--dur-fast) var(--ease-flit)",
      }}
    >
      <Icon name={icon} size={20} />
      {/* Sans, not the 11px mono meta style: a nav label is a word a human
          wrote, and the meta style means "a value the system computed". */}
      <span style={{ font: "var(--weight-medium) 11px/1 var(--font-sans)" }}>{label}</span>
      {count ? (
        <Badge
          tone={danger ? "danger" : "neutral"}
          mono
          style={{
            position: "absolute",
            top: 2,
            insetInlineStart: "calc(50% + 4px)",
            height: 15,
            padding: "0 var(--space-2)",
            fontSize: 10,
          }}
        >
          {count}
        </Badge>
      ) : null}
    </button>
  );
}

/** The overflow sheet: the five screens the bar cannot hold, the theme toggle
 * that has no rail footer to sit in any more, and the build version — which on
 * a phone is reachable here and in Settings, and nowhere else.
 *
 * Mounted only while open, so mount is open and unmount is close — which is
 * what lets the focus contract below be one effect with no dependencies. */
function MoreSheet({
  screen,
  counts,
  statusLabel,
  theme,
  onToggleTheme,
  onScreen,
  onClose,
}: {
  screen: Screen;
  counts: Partial<Record<Screen, number>>;
  statusLabel: string;
  theme: ResolvedTheme;
  onToggleTheme: () => void;
  onScreen: (screen: Screen) => void;
  onClose: () => void;
}) {
  const restoreTo = useRef<Element | null>(null);
  const panelRef = useRef<HTMLDivElement>(null);

  // Focus in, and back out again — `CapturePopover`'s contract, and the same
  // two halves. `aria-modal="true"` tells assistive tech to ignore everything
  // outside this panel, and the More button that opened it is outside: leaving
  // focus there parks a VoiceOver user on an element that has just been hidden
  // from them, with the sheet they asked for unreachable. So the panel itself
  // takes focus (hence `tabIndex={-1}` on it, which makes a container
  // focusable without putting it in the tab order) rather than its first
  // control: that way the sheet is announced by its own name before its
  // contents, the way a sheet opening should read.
  //
  // The restore half is `CapturePopover`'s verbatim, and matters more here
  // than it does there — every way out of this sheet is a way *back to the
  // bar*, so without it a keyboard user is dropped at the top of the document
  // after every visit to More. `isConnected` because a chosen screen may have
  // re-rendered the bar out from under the node we saved.
  //
  // No focus trap, deliberately, and for the same reason `CapturePopover`
  // has none: Escape is bound to the document (above), so tabbing out of the
  // panel does not strand anyone — the sheet still closes and still hands
  // focus back. A trap here would be a second idiom for the same job.
  useEffect(() => {
    restoreTo.current = document.activeElement;
    panelRef.current?.focus();
    return () => {
      const target = restoreTo.current;
      if (target instanceof HTMLElement && target.isConnected) {
        target.focus();
      }
    };
  }, []);

  return (
    // The scrim. `onMouseDown`, and only when the press landed on the scrim
    // itself: a press that starts inside the sheet and ends outside it is not
    // a request to close (`CapturePopover`'s rule). `presentation` because it
    // is a backdrop over two real, focusable ways out — Escape and Close.
    <div
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) {
          onClose();
        }
      }}
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 40,
        display: "flex",
        flexDirection: "column",
        justifyContent: "flex-end",
        background: "var(--surface-scrim)",
      }}
    >
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-label="More surfaces"
        tabIndex={-1}
        style={{
          background: "var(--surface-card)",
          borderTop: "1px solid var(--border-subtle)",
          // The panel is a focus *destination*, not a control, and a ring
          // drawn around the whole sheet says nothing a sighted user did not
          // already see happen. Every control inside keeps its own.
          outline: "none",
          // A sheet's radius (--radius-2xl), top corners only — it rises from
          // the bottom edge, so the bottom two never show.
          borderRadius: "var(--radius-2xl) var(--radius-2xl) 0 0",
          boxShadow: "var(--shadow-3)",
          padding: "var(--space-6) var(--gutter-page)",
          paddingBottom: "calc(var(--space-6) + env(safe-area-inset-bottom))",
          display: "flex",
          flexDirection: "column",
          gap: "var(--space-5)",
          maxHeight: "80dvh",
          overflowY: "auto",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: "var(--space-5)" }}>
          <h2 style={{ flex: 1, minWidth: 0, font: "var(--type-h3)", color: "var(--text-primary)" }}>
            More
          </h2>
          <IconButton icon="x" label="Close" onClick={onClose} />
        </div>

        <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
          {NAV_BAR_OVERFLOW.map((item) => {
            const label = SCREEN_LABELS[item];
            const icon = SCREEN_ICONS[item];
            const active = item === screen;
            const count = counts[item];
            return (
              <button
                key={item}
                type="button"
                onClick={() => onScreen(item)}
                aria-label={label}
                aria-current={active ? "page" : undefined}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: "var(--space-5)",
                  // The design system's row height, which is also --touch-min.
                  minHeight: "var(--row-height)",
                  padding: "0 var(--space-4)",
                  background: active ? "var(--accent-quiet)" : "transparent",
                  color: active ? "var(--text-brand)" : "var(--text-primary)",
                  border: "1px solid transparent",
                  borderRadius: "var(--radius-control)",
                  font: `var(--weight-${active ? "semibold" : "medium"}) var(--size-body)/1 var(--font-sans)`,
                  cursor: "pointer",
                  textAlign: "left",
                }}
              >
                <Icon name={icon} size={18} />
                <span style={{ flex: 1 }}>{label}</span>
                {count ? (
                  <Badge tone={item === "alerts" ? "danger" : "neutral"} mono>
                    {count}
                  </Badge>
                ) : null}
              </button>
            );
          })}
        </div>

        {/* The rail's footer, rehoused: the two meta lines and the theme
            toggle, which has nowhere else to live on a phone. */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: "var(--space-5)",
            paddingTop: "var(--space-5)",
            borderTop: "1px solid var(--border-subtle)",
          }}
        >
          <ShellMeta statusLabel={statusLabel} style={{ flex: 1 }} />
          <IconButton
            icon={theme === "dark" ? "sun" : "moon"}
            label={theme === "dark" ? "Switch to light theme" : "Switch to dark theme"}
            onClick={onToggleTheme}
            style={{ flex: "0 0 auto" }}
          />
        </div>
      </div>
    </div>
  );
}
