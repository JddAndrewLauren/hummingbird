const { Card, Button, Badge, ItemRow, ContextTile, StageBadge, Icon } = window.HummingbirdDesignSystem_dcdceb;

function SnapshotTile({ s }) {
  return (
    <Card padding="var(--space-5)" style={{ flex: 1, display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
      <span className="hb-meta">{s.name}</span>
      <span style={{ font: "var(--weight-bold) 22px/1.1 var(--font-display)", color: "var(--text-primary)" }}>{s.value}</span>
      <span className="hb-meta" style={{ color: "var(--text-muted)" }}>{s.note}</span>
    </Card>
  );
}

function NowScreen({ data, onOpen }) {
  const top = data.items[1];
  const rest = data.items.filter((i) => i.id !== top.id && i.stage !== "done");
  return (
    <div style={{ display: "flex", gap: "var(--space-8)", alignItems: "flex-start", flexWrap: "wrap" }}>
      <div style={{ flex: 1, minWidth: 380, display: "flex", flexDirection: "column", gap: "var(--space-7)" }}>
        <div>
          <span className="hb-meta">top pick · thursday</span>
          <Card accent elevation={2} padding="var(--space-7)" style={{ marginTop: "var(--space-4)", display: "flex", flexDirection: "column", gap: "var(--space-5)" }}>
            <div style={{ display: "flex", alignItems: "center", gap: "var(--space-4)" }}>
              <StageBadge stage={top.stage} />
              <Badge mono>size:{top.size}</Badge>
              <Badge mono tone="brand">{top.id}</Badge>
            </div>
            <h2 style={{ font: "var(--type-h1)", letterSpacing: "var(--tracking-heading)" }}>{top.title}</h2>
            <p style={{ font: "var(--type-body)", color: "var(--text-secondary)" }}>
              Started 40 minutes ago. Three of seven steps ticked; the next one is "regenerate the fixture set".
            </p>
            <div style={{ display: "flex", gap: "var(--space-4)", flexWrap: "wrap" }}>
              <Button iconLeft="play">Resume</Button>
              <Button variant="secondary" iconLeft="list-checks" onClick={() => onOpen("routes")}>Steps ({top.steps})</Button>
              <Button variant="ghost" iconLeft="clock">Later today</Button>
            </div>
          </Card>
        </div>
        <div>
          <div style={{ display: "flex", alignItems: "baseline", justifyContent: "space-between", marginBottom: "var(--space-4)" }}>
            <h3 style={{ font: "var(--type-h3)" }}>Also startable</h3>
            <span className="hb-meta">{rest.length} actions</span>
          </div>
          <Card padding="var(--space-3)">
            {rest.map((i) => <ItemRow key={i.id} {...i} onClick={() => onOpen("routes")} />)}
          </Card>
        </div>
      </div>
      <aside style={{ width: "var(--panel-width)", flex: "0 0 auto", display: "flex", flexDirection: "column", gap: "var(--space-6)" }}>
        <div>
          <span className="hb-meta">calendar context</span>
          <div style={{ marginTop: "var(--space-4)", display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
            <ContextTile kind="in_progress" title="Design review" timeLabel="9:30–10:00 AM" href="#" asOf="just now" />
            <ContextTile kind="upcoming" title="School pickup" timeLabel="3:10–3:30 PM" asOf="42m ago" stale />
          </div>
        </div>
        <div>
          <span className="hb-meta">standing questions</span>
          <Card padding="var(--space-5)" style={{ marginTop: "var(--space-4)", display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
            {[["What's this weekend?", "Two things scheduled, nothing due."], ["How long to the next vacation?", "38 days."]].map(([q, a]) => (
              <div key={q} style={{ display: "flex", gap: "var(--space-4)" }}>
                <Icon name="help-circle" size={16} color="var(--text-muted)" style={{ marginTop: 2 }} />
                <div>
                  <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>{q}</p>
                  <p style={{ font: "var(--type-body-strong)" }}>{a}</p>
                </div>
              </div>
            ))}
          </Card>
        </div>
        <div>
          <span className="hb-meta">context snapshots</span>
          <div style={{ marginTop: "var(--space-4)", display: "flex", gap: "var(--space-4)", flexWrap: "wrap" }}>
            {data.snapshots.map((s) => <SnapshotTile key={s.name} s={s} />)}
          </div>
        </div>
      </aside>
    </div>
  );
}
Object.assign(window, { NowScreen });
