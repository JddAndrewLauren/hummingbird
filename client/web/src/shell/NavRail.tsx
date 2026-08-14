import { Badge } from "../components/core/Badge";
import { Icon } from "../components/core/Icon";
import { IconButton } from "../components/core/IconButton";
import markDark1x from "../design/brand/app-icon-dark-26.png";
import markDark2x from "../design/brand/app-icon-dark-52.png";
import markDark3x from "../design/brand/app-icon-dark-78.png";
import markLight1x from "../design/brand/app-icon-light-26.png";
import markLight2x from "../design/brand/app-icon-light-52.png";
import markLight3x from "../design/brand/app-icon-light-78.png";
import type { ResolvedTheme } from "../theme/theme";
import { SCREEN_ICONS } from "./screen-icons";
import { SCREEN_LABELS, SCREENS, type Screen } from "./screens";
import { ShellMeta } from "./ShellMeta";

export interface NavRailProps {
  screen: Screen;
  onScreen: (screen: Screen) => void;
  /** Per-screen counts. Absent counts render no badge — a zero pill would
   * be decoration, and an invented one would be a lie. */
  counts?: Partial<Record<Screen, number>>;
  /** The core's state, already formatted (see status-label.ts). */
  statusLabel: string;
  theme: ResolvedTheme;
  onToggleTheme: () => void;
  /** Collapsed shows icons and counts only — labels, wordmark and status
   * line all go, and every button keeps its name via `aria-label`/`title`.
   * The state is the caller's (`App.tsx` persists it device-locally via
   * `rail-collapse.ts`). */
  collapsed: boolean;
  onToggleCollapsed: () => void;
  /** The mark is the way home: it selects Now and refreshes, the same two
   * things the user would otherwise do by hand. */
  onHome: () => void;
}

/** Collapsed width: one 36px control row centred inside the same
 * `--space-5` horizontal padding the expanded rail uses. Local by design —
 * `--rail-width` (236px) is the design system's constant; this is the
 * shell's own compact form of it. */
const COLLAPSED_WIDTH = 68;

export function NavRail({
  screen,
  onScreen,
  counts = {},
  statusLabel,
  theme,
  onToggleTheme,
  collapsed,
  onToggleCollapsed,
  onHome,
}: NavRailProps) {
  return (
    <nav
      aria-label="Surfaces"
      style={{
        display: "flex",
        flexDirection: "column",
        gap: "var(--space-6)",
        flex: "0 0 auto",
        width: collapsed ? COLLAPSED_WIDTH : "var(--rail-width)",
        padding: "var(--space-6) var(--space-5)",
        background: "var(--surface-quiet)",
        borderRight: "1px solid var(--border-subtle)",
        transition: "width var(--dur-base) var(--ease-flit)",
        overflow: "hidden",
      }}
    >
      {/* Mark + wordmark, as the design system's own NavRail draws it: the app
          icon at 26px, squircled with --radius-icon-app, and no plate or
          border of its own ("never on a coloured plate of its own").
          See design/brand/README.md for where the artwork comes from.
          The collapse control sits here rather than in the footer: it acts on
          the whole rail, so it belongs beside the rail's own title. */}
      <div
        style={{
          display: "flex",
          flexDirection: collapsed ? "column" : "row",
          alignItems: "center",
          gap: "var(--space-4)",
          padding: collapsed ? 0 : "0 var(--space-3)",
          justifyContent: collapsed ? "center" : "flex-start",
        }}
      >
        {/* The mark and the wordmark are one control — the way home. The img
            keeps alt="" because the button is what carries the name. */}
        <button
          type="button"
          onClick={onHome}
          aria-label="hummingbird — go to Now and refresh"
          title="Go to Now and refresh"
          style={{
            display: "flex",
            alignItems: "center",
            gap: "var(--space-4)",
            flex: collapsed ? "0 0 auto" : 1,
            minWidth: 0,
            padding: 0,
            background: "transparent",
            border: "1px solid transparent",
            borderRadius: "var(--radius-control)",
            cursor: "pointer",
            textAlign: "left",
          }}
        >
          <img
            src={theme === "dark" ? markDark1x : markLight1x}
            // Raster art, so hidpi needs real pixels rather than an upscale.
            srcSet={
              theme === "dark"
                ? `${markDark1x} 1x, ${markDark2x} 2x, ${markDark3x} 3x`
                : `${markLight1x} 1x, ${markLight2x} 2x, ${markLight3x} 3x`
            }
            alt=""
            width={26}
            height={26}
            style={{
              display: "block",
              flex: "0 0 auto",
              borderRadius: "var(--radius-icon-app)",
            }}
          />
          {collapsed ? null : (
            <span
              style={{
                font: "var(--weight-bold) 18px/1 var(--font-display)",
                letterSpacing: "-0.03em",
                color: "var(--text-primary)",
              }}
            >
              hummingbird
            </span>
          )}
        </button>
        {/* One chevron, rotated — `TriageRow`'s own idiom: pointing away
            from the content to collapse, toward it to expand, and the label
            says it in words. */}
        <IconButton
          icon="chevron-down"
          label={collapsed ? "Expand the sidebar" : "Collapse the sidebar"}
          size="sm"
          onClick={onToggleCollapsed}
          style={{
            flex: "0 0 auto",
            width: 30,
            height: 30,
            transform: collapsed ? "rotate(-90deg)" : "rotate(90deg)",
            transition: "transform var(--dur-base) var(--ease-flit)",
          }}
        />
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
        {SCREENS.map((item) => {
          const label = SCREEN_LABELS[item];
          const icon = SCREEN_ICONS[item];
          const active = item === screen;
          const count = counts[item];
          return (
            <button
              key={item}
              type="button"
              onClick={() => onScreen(item)}
              aria-current={active ? "page" : undefined}
              // Always named, so the collapsed (label-less) button reads the
              // same to a screen reader; `title` is the hover tooltip the
              // collapsed form needs.
              aria-label={label}
              title={collapsed ? label : undefined}
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: collapsed ? "center" : "flex-start",
                gap: "var(--space-4)",
                height: 36,
                padding: collapsed ? 0 : "0 var(--space-3)",
                position: "relative",
                background: active ? "var(--accent-quiet)" : "transparent",
                color: active ? "var(--text-brand)" : "var(--text-secondary)",
                border: "1px solid transparent",
                borderRadius: "var(--radius-control)",
                font: `var(--weight-${active ? "semibold" : "medium"}) var(--size-body)/1 var(--font-sans)`,
                cursor: "pointer",
                textAlign: "left",
                transition: "background var(--dur-fast) var(--ease-flit), color var(--dur-fast) var(--ease-flit)",
              }}
            >
              <Icon name={icon} size={17} />
              {collapsed ? null : <span style={{ flex: 1 }}>{label}</span>}
              {count ? (
                <Badge
                  tone={item === "alerts" ? "danger" : "neutral"}
                  mono
                  // Collapsed keeps the count — the whole point of the badge
                  // — as a compact pill pinned to the icon's corner.
                  style={
                    collapsed
                      ? {
                          position: "absolute",
                          top: 1,
                          right: 3,
                          height: 15,
                          padding: "0 var(--space-2)",
                          fontSize: 10,
                        }
                      : undefined
                  }
                >
                  {count}
                </Badge>
              ) : null}
            </button>
          );
        })}
      </div>

      <div
        style={{
          marginTop: "auto",
          display: "flex",
          flexDirection: collapsed ? "column" : "row",
          alignItems: "center",
          gap: "var(--space-4)",
          padding: collapsed ? 0 : "0 var(--space-3)",
        }}
      >
        {/* The core's state over the build version — `ShellMeta` owns both
            lines and the argument for stacking them. Shared with the phone
            form's More sheet, which is the only place the version is
            reachable there. */}
        {collapsed ? null : <ShellMeta statusLabel={statusLabel} style={{ flex: 1 }} />}
        <IconButton
          icon={theme === "dark" ? "sun" : "moon"}
          label={theme === "dark" ? "Switch to light theme" : "Switch to dark theme"}
          size="sm"
          onClick={onToggleTheme}
          style={{ flex: "0 0 auto", width: 30, height: 30 }}
        />
      </div>
    </nav>
  );
}
