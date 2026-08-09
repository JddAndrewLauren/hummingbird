const { Card, AlertCard, Button, Badge, Switch, EmptyState, Icon } = window.HummingbirdDesignSystem_dcdceb;

function AlertsScreen({ data }) {
  const [acked, setAcked] = React.useState({});
  const live = data.alerts.filter((a) => !acked[a.id]);
  return (
    <div style={{ display: "flex", gap: "var(--space-8)", alignItems: "flex-start", flexWrap: "wrap" }}>
      <div style={{ flex: 1, minWidth: 380, display: "flex", flexDirection: "column", gap: "var(--space-5)" }}>
        <div style={{ display: "flex", alignItems: "baseline", justifyContent: "space-between" }}>
          <h3 style={{ font: "var(--type-h3)" }}>Live</h3>
          <span className="hb-meta">{live.length} unacked · default-deny</span>
        </div>
        {live.length === 0
          ? <Card padding="0"><EmptyState icon="bell-off" headingLevel={3} title="Nothing is ringing" body="Every alert has been acked. What no rule matches stays silent." /></Card>
          : data.alerts.map((a) => (
              <AlertCard key={a.id} {...a} acked={!!acked[a.id]} href="#"
                onAck={() => setAcked((s) => ({ ...s, [a.id]: true }))} />
            ))}
      </div>
      <aside style={{ width: "var(--panel-width)", flex: "0 0 auto", display: "flex", flexDirection: "column", gap: "var(--space-5)" }}>
        <span className="hb-meta">rules</span>
        {[["Sweeper failures", "urgent", "Any hb-worker run that exits non-zero."],
          ["Deploys", "normal", "Cloudflare deploy events on hb.twinion.net."],
          ["Gmail capture label", "normal", "Mail tagged capture, hourly digest."]].map(([n, tier, d]) => (
          <Card key={n} padding="var(--space-5)" style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
              <span style={{ font: "var(--type-body-strong)" }}>{n}</span>
              <Badge mono tone={tier === "urgent" ? "danger" : "info"}>{tier}</Badge>
            </div>
            <span style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>{d}</span>
          </Card>
        ))}
        <Card padding="var(--space-5)"><Switch checked label="Urgent tier only on the watch" hint="Normal-tier deliveries stay on phone and desktop." /></Card>
      </aside>
    </div>
  );
}
Object.assign(window, { AlertsScreen });
