const { Icon, Badge } = window.HummingbirdDesignSystem_dcdceb;

const NAV = [
  { id: "now", label: "Now", icon: "zap" },
  { id: "triage", label: "Triage", icon: "inbox", count: 4 },
  { id: "routes", label: "Routes", icon: "route" },
  { id: "alerts", label: "Alerts", icon: "bell", count: 1 },
  { id: "settings", label: "Settings", icon: "settings" },
];

function NavRail({ screen, onScreen, theme, onTheme }) {
  return (
    <nav style={{ width: "var(--rail-width)", flex: "0 0 auto", display: "flex", flexDirection: "column",
      gap: "var(--space-2)", padding: "var(--space-6) var(--space-5)", background: "var(--surface-quiet)",
      borderRight: "1px solid var(--border-subtle)" }}>
      <div style={{ display: "flex", alignItems: "center", gap: "var(--space-4)", padding: "var(--space-2) var(--space-4) var(--space-7)" }}>
        <img src="../../assets/app-icon-light-1024.png" width="26" height="26" style={{ borderRadius: "22.37%" }} alt="" />
        <span style={{ font: "var(--weight-bold) 18px/1 var(--font-display)", letterSpacing: "-0.03em" }}>hummingbird</span>
      </div>
      {NAV.map((n) => {
        const on = screen === n.id;
        return (
          <button key={n.id} onClick={() => onScreen(n.id)}
            style={{ display: "flex", alignItems: "center", gap: "var(--space-5)", height: 36,
              padding: "0 var(--space-4)", border: "none", cursor: "pointer", textAlign: "left",
              background: on ? "var(--accent-quiet)" : "transparent",
              color: on ? "var(--text-brand)" : "var(--text-secondary)",
              borderRadius: "var(--radius-control)", font: "var(--weight-semibold) var(--size-body)/1 var(--font-sans)",
              transition: "background var(--dur-fast) var(--ease-flit)" }}>
            <Icon name={n.icon} size={17} />
            <span style={{ flex: 1 }}>{n.label}</span>
            {n.count ? <Badge mono tone={n.id === "alerts" ? "danger" : "neutral"}>{n.count}</Badge> : null}
          </button>
        );
      })}
      <div style={{ marginTop: "auto", display: "flex", alignItems: "center", justifyContent: "space-between", gap: "var(--space-4)" }}>
        <span className="hb-meta">core ready · api v1</span>
        <button onClick={onTheme} aria-label="Toggle theme"
          style={{ display: "inline-flex", alignItems: "center", justifyContent: "center", width: 30, height: 30,
            background: "transparent", border: "1px solid var(--border-default)", borderRadius: "var(--radius-control)",
            color: "var(--text-secondary)", cursor: "pointer" }}>
          <Icon name={theme === "dark" ? "sun" : "moon"} size={15} />
        </button>
      </div>
    </nav>
  );
}
Object.assign(window, { NavRail });
