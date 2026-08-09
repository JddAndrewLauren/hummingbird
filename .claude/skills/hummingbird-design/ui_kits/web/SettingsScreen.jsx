const { Card, Button, CalendarPicker, Switch, Select, Badge, Icon, ContextTile } = window.HummingbirdDesignSystem_dcdceb;

// The right-hand column is a faithful recreation of the shipped placeholder
// shell (client/web/src/App.tsx @ d4105b5) — the only Hummingbird UI that
// exists in the repository today.
function ShippedShell() {
  return (
    <div style={{ background: "#020617", color: "#f1f5f9", borderRadius: "var(--radius-card)",
      padding: "32px", display: "flex", flexDirection: "column", alignItems: "center", gap: "16px",
      font: "400 14px/1.5 ui-sans-serif, system-ui, sans-serif" }}>
      <h1 style={{ font: "600 24px/1.2 ui-sans-serif, system-ui, sans-serif" }}>hummingbird</h1>
      <p>Core ready (api v1) — worker + wasm loaded.</p>
      <div style={{ display: "flex", flexDirection: "column", gap: "12px", width: "100%", maxWidth: "336px" }}>
        <div style={{ border: "1px solid #1e293b", borderRadius: "8px", padding: "16px" }}>
          <p style={{ fontSize: "12px", textTransform: "uppercase", letterSpacing: ".05em", color: "#94a3b8" }}>Now</p>
          <a href="#" style={{ fontWeight: 500, textDecoration: "underline", color: "#f1f5f9" }}>Design review</a>
          <p style={{ fontSize: "12px", color: "#94a3b8" }}>9:30 AM–10:00 AM</p>
          <p style={{ marginTop: "8px", fontSize: "12px", color: "#64748b" }}>as of just now</p>
        </div>
        <button style={{ borderRadius: "6px", border: "1px solid #1e293b", background: "transparent",
          color: "#cbd5e1", padding: "8px 12px", fontSize: "14px", fontWeight: 500 }}>Refresh calendar</button>
      </div>
    </div>
  );
}

function SettingsScreen({ data }) {
  const [sel, setSel] = React.useState(["andrew@…", "family@group.calendar.google.com"]);
  return (
    <div style={{ display: "grid", gridTemplateColumns: "1fr 380px", gap: "var(--space-8)", alignItems: "start" }}>
      <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-6)" }}>
        <div>
          <h3 style={{ font: "var(--type-h3)", marginBottom: "var(--space-4)" }}>Calendar context</h3>
          <CalendarPicker calendars={data.calendars} selectedIds={sel}
            unavailableIds={["team@group.calendar.google.com"]}
            onToggle={(id) => setSel((s) => s.includes(id) ? s.filter((x) => x !== id) : [...s, id])} />
        </div>
        <Card padding="var(--space-6)" style={{ display: "flex", flexDirection: "column", gap: "var(--space-5)" }}>
          <span className="hb-meta">this device</span>
          <Switch checked label="Poll while the tab is backgrounded" hint="Off: the 15-minute timer only ticks while foregrounded and online." />
          <Switch label="Show acked alerts in the list" />
          <Select label="Theme" options={["Follow system", "Light", "Dark"]} defaultValue="Follow system" />
        </Card>
        <Card padding="var(--space-6)" style={{ display: "flex", alignItems: "center", gap: "var(--space-5)" }}>
          <Icon name="database" size={18} color="var(--text-muted)" />
          <div style={{ flex: 1 }}>
            <p style={{ font: "var(--type-body-strong)" }}>Mirror</p>
            <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>Derived and disposable. Deleting it loses nothing.</p>
          </div>
          <Button variant="secondary" size="sm" iconLeft="rotate-ccw">Rebuild</Button>
        </Card>
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
        <span className="hb-meta">shipped today · client/web App.tsx</span>
        <ShippedShell />
        <p style={{ font: "var(--type-body-sm)", color: "var(--text-muted)" }}>
          The live web client is a placeholder shell on Tailwind defaults. Everything else in this kit is proposed, built from the domain vocabulary in CONTEXT.md.
        </p>
      </div>
    </div>
  );
}
Object.assign(window, { SettingsScreen });
