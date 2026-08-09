import { Badge } from "../components/core/Badge";
import { Icon, type IconName } from "../components/core/Icon";
import { IconButton } from "../components/core/IconButton";
import markDark from "../design/brand/hummingbird-icon-micro-dark.svg";
import markLight from "../design/brand/hummingbird-icon-micro-light.svg";
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
      {/* Mark + wordmark. The mark is the app icon's `micro` optical profile,
          the variant generated for 24–16px reads — see design/brand/README.md
          for provenance and why it is an <img> rather than inline SVG. It is
          decorative here: the wordmark beside it already names the app, so
          alt="" keeps screen readers from announcing the brand twice. */}
      <div style={{ display: "flex", alignItems: "center", gap: "var(--space-4)", padding: "0 var(--space-3)" }}>
        <img
          src={theme === "dark" ? markDark : markLight}
          alt=""
          width={22}
          height={22}
          style={{
            display: "block",
            flex: "0 0 auto",
            borderRadius: "var(--radius-icon-app)",
            // The light plate (#FBF7F0) and the rail behind it
            // (--surface-quiet, #faf0e7) are the same value to the eye, so
            // without an edge the plate vanishes in the light theme and the
            // icon's deliberately-cropped composition reads as a shape cut
            // off mid-body. A hairline gives it its boundary back. Inset
            // rather than a border so the box stays 22px and the ring
            // follows the squircle exactly.
            boxShadow: "inset 0 0 0 1px var(--border-subtle)",
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
