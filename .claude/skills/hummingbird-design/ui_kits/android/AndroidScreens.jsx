const { Button, IconButton, Badge, Card, ItemRow, StageBadge, ContextTile, AlertCard, Icon, Input, EmptyState, Slider, Select } = window.HummingbirdDesignSystem_dcdceb;

const HB_CONTEXTS_AND = ["@home","@computer","@phone","@errands","@garden","@waiting"];

const PAD = { padding: "44px var(--space-6) 30px", display: "flex", flexDirection: "column", gap: "var(--space-6)", background: "var(--surface-page)", minHeight: "100%", position: "relative" };

function TopBar({ title, action }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: "var(--space-5)" }}>
      <img src="../../assets/app-icon-light-1024.png" width="24" height="24" style={{ borderRadius: "22.37%" }} alt="" />
      <span style={{ flex: 1, font: "var(--weight-bold) 20px/1.2 var(--font-display)", letterSpacing: "-0.02em" }}>{title}</span>
      {action}
    </div>
  );
}

function Fab({ icon = "feather", label = "Capture" }) {
  return (
    <button aria-label={label} style={{ position: "absolute", right: 18, bottom: 46, display: "inline-flex",
      alignItems: "center", gap: "var(--space-4)", height: 56, padding: "0 var(--space-7)",
      background: "var(--accent)", color: "var(--on-accent)", border: "none",
      borderRadius: "var(--radius-xl)", boxShadow: "var(--shadow-accent)", cursor: "pointer",
      font: "var(--weight-semibold) var(--size-body)/1 var(--font-sans)" }}>
      <Icon name={icon} size={20} />{label}
    </button>
  );
}

function AndroidNow() {
  return (
    <div style={PAD}>
      <TopBar title="hummingbird" action={<IconButton icon="search" label="Search" size="lg" />} />
      <ContextTile kind="upcoming" title="School pickup" timeLabel="3:10–3:30 PM" asOf="42m ago" stale />
      <div>
        <span className="hb-meta">today · 4 startable</span>
        <Card padding="var(--space-3)" style={{ marginTop: "var(--space-4)" }}>
          <ItemRow title="Rewrite the sweeper's Gmail adapter" stage="in_progress" urgency="now" steps="3/7" selected />
          <ItemRow title="Order the replacement sensor" stage="ready" urgency="soon" deadline="Fri" />
          <ItemRow title="Book the annual boiler service" stage="ready" urgency="calm" scheduled="Mon" />
          <ItemRow title="Hear back from the shop" stage="blocked" urgency="calm" blockedBy="ION-142" />
        </Card>
      </div>
      <Fab />
    </div>
  );
}

function AndroidAlerts() {
  const [acked, setAcked] = React.useState(false);
  return (
    <div style={PAD}>
      <TopBar title="Alerts" action={<IconButton icon="sliders-horizontal" label="Rules" size="lg" />} />
      <AlertCard tier="urgent" source="Fly · hb-worker" title="Sweeper run failed"
        detail="Google Tasks adapter returned 503 twice." time="6m" acked={acked} onAck={() => setAcked(true)} href="#" />
      <AlertCard tier="normal" source="Cloudflare · hb.twinion.net" title="Deploy succeeded" detail="client/web @ d4105b5." time="1h" href="#" />
      <Card padding="var(--space-5)" style={{ background: "var(--surface-quiet)", display: "flex", gap: "var(--space-4)", alignItems: "center" }}>
        <Icon name="shield" size={16} color="var(--text-muted)" />
        <span style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>Default-deny: what no rule matches stays silent.</span>
      </Card>
    </div>
  );
}

function AndroidTriage() {
  const [energy, setEnergy] = React.useState(2);
  const [size, setSize] = React.useState(null);
  const [ctx, setCtx] = React.useState("@home");
  return (
    <div style={{ ...PAD, justifyContent: "flex-end", paddingBottom: 0 }}>
      <div style={{ position: "absolute", inset: 0, background: "var(--surface-scrim)" }} />
      <div style={{ position: "relative", background: "var(--surface-card)", borderRadius: "var(--radius-2xl) var(--radius-2xl) 0 0",
        padding: "var(--space-6) var(--space-6) var(--space-11)", display: "flex", flexDirection: "column", gap: "var(--space-6)",
        boxShadow: "var(--shadow-3)" }}>
        <div style={{ width: 32, height: 4, borderRadius: 2, background: "var(--border-strong)", alignSelf: "center" }} />
        <span style={{ font: "var(--type-h3)" }}>Capture</span>
        <Input icon="feather" placeholder="What's on your mind?" size="lg" defaultValue="the fence gate is dragging again" />
        <Slider label="Energy" options={["low", "medium", "high"]} value={energy} onChange={setEnergy} />
        <Slider label="Size" options={["quick", "normal", "deep"]} value={size} onChange={setSize} />
        <Select label="Context" size="lg" value={ctx} onChange={(e) => setCtx(e.target.value)}
          options={[{ value: "", label: "Not set" }, ...HB_CONTEXTS_AND.map((c) => ({ value: c, label: c }))]} />
        <div style={{ display: "flex", gap: "var(--space-4)" }}>
          <Badge tone="neutral" icon="folder">House</Badge><Badge tone="neutral" icon="mic">Voice</Badge>
        </div>
        <Button size="lg" fullWidth iconLeft="plus">Add to Triage</Button>
      </div>
    </div>
  );
}
Object.assign(window, { AndroidNow, AndroidAlerts, AndroidTriage });
