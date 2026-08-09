import { Badge } from "../components/core/Badge";
import { Icon, type IconName } from "../components/core/Icon";
import { IconButton } from "../components/core/IconButton";
import markDark1x from "../design/brand/app-icon-dark-26.png";
import markDark2x from "../design/brand/app-icon-dark-52.png";
import markDark3x from "../design/brand/app-icon-dark-78.png";
import markLight1x from "../design/brand/app-icon-light-26.png";
import markLight2x from "../design/brand/app-icon-light-52.png";
import markLight3x from "../design/brand/app-icon-light-78.png";
import type { ResolvedTheme } from "../theme/theme";
import { SCREENS, type Screen } from "./screens";

// Nav labels are the surface's own name; the header asks the question the
// screen answers (see SCREEN_TITLES). The order is `SCREENS`, not a second
// list here — this record only says how each one is drawn, and being a
// `Record<Screen, …>` it cannot silently miss a screen.
const NAV: Record<Screen, { label: string; icon: IconName }> = {
  now: { label: "Now", icon: "zap" },
  triage: { label: "Triage", icon: "inbox" },
  routes: { label: "Routes", icon: "route" },
  alerts: { label: "Alerts", icon: "bell" },
  settings: { label: "Settings", icon: "settings" },
};

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
}

export function NavRail({
  screen,
  onScreen,
  counts = {},
  statusLabel,
  theme,
  onToggleTheme,
}: NavRailProps) {
  return (
    <nav
      aria-label="Surfaces"
      style={{
        display: "flex",
        flexDirection: "column",
        gap: "var(--space-6)",
        flex: "0 0 auto",
        width: "var(--rail-width)",
        padding: "var(--space-6) var(--space-5)",
        background: "var(--surface-quiet)",
        borderRight: "1px solid var(--border-subtle)",
      }}
    >
      {/* Mark + wordmark, as the design system's own NavRail draws it: the app
          icon at 26px, squircled with --radius-icon-app, and no plate or
          border of its own ("never on a coloured plate of its own"). The mark
          is decorative — the wordmark beside it already names the app, so
          alt="" keeps screen readers from announcing the brand twice.
          See design/brand/README.md for where the artwork comes from. */}
      <div style={{ display: "flex", alignItems: "center", gap: "var(--space-4)", padding: "0 var(--space-3)" }}>
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
        <span
          style={{
            font: "var(--weight-bold) 18px/1 var(--font-display)",
            letterSpacing: "-0.03em",
            color: "var(--text-primary)",
          }}
        >
          hummingbird
        </span>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
        {SCREENS.map((item) => {
          const { label, icon } = NAV[item];
          const active = item === screen;
          const count = counts[item];
          return (
            <button
              key={item}
              type="button"
              onClick={() => onScreen(item)}
              aria-current={active ? "page" : undefined}
              style={{
                display: "flex",
                alignItems: "center",
                gap: "var(--space-4)",
                height: 36,
                padding: "0 var(--space-3)",
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

      <div
        style={{
          marginTop: "auto",
          display: "flex",
          alignItems: "center",
          gap: "var(--space-4)",
          padding: "0 var(--space-3)",
        }}
      >
        <span className="hb-meta" style={{ flex: 1 }}>
          {statusLabel}
        </span>
        <IconButton
          icon={theme === "dark" ? "sun" : "moon"}
          label={theme === "dark" ? "Switch to light theme" : "Switch to dark theme"}
          size="sm"
          onClick={onToggleTheme}
          style={{ width: 30, height: 30 }}
        />
      </div>
    </nav>
  );
}
