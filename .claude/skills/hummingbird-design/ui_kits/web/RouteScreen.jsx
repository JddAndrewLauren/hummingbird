const { Card, Button, Badge, ItemRow, Checkbox, Icon, StageBadge } = window.HummingbirdDesignSystem_dcdceb;

function RouteScreen({ data }) {
  const [steps, setSteps] = React.useState([
    { t: "Open the fixture generator script", done: true },
    { t: "Regenerate the Gmail fixture set", done: true },
    { t: "Run the adapter tests once", done: true },
    { t: "Delete the two dead label cases", done: false },
    { t: "Re-read the sweeper doc's invariants", done: false },
  ]);
  const route = data.route;
  const actions = data.items.filter((i) => route.actions.includes(i.id));
  return (
    <div style={{ display: "flex", gap: "var(--space-8)", alignItems: "flex-start", flexWrap: "wrap" }}>
      <div style={{ flex: 1, minWidth: 380, display: "flex", flexDirection: "column", gap: "var(--space-7)" }}>
        <div>
          <span className="hb-meta">route · {route.project}</span>
          <h2 style={{ font: "var(--type-h1)", letterSpacing: "var(--tracking-heading)", marginTop: "var(--space-4)" }}>Destination</h2>
          <p style={{ font: "var(--type-body)", color: "var(--text-secondary)", marginTop: "var(--space-3)", maxWidth: 560 }}>{route.destination}</p>
        </div>
        <div>
          <h3 style={{ font: "var(--type-h3)", marginBottom: "var(--space-4)" }}>Minted actions</h3>
          <Card padding="var(--space-3)">{actions.map((i) => <ItemRow key={i.id} {...i} />)}</Card>
        </div>
        <div>
          <h3 style={{ font: "var(--type-h3)", marginBottom: "var(--space-4)" }}>Fog</h3>
          {route.fog.map((f) => (
            <Card key={f.q} padding="var(--space-5)" style={{ display: "flex", gap: "var(--space-5)", borderColor: "var(--stage-grilling)" }}>
              <Icon name="cloud-fog" size={18} color="var(--stage-grilling)" style={{ marginTop: 2 }} />
              <div>
                <p style={{ font: "var(--type-body-strong)" }}>{f.q}</p>
                <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)", marginTop: 2 }}>{f.note}</p>
              </div>
            </Card>
          ))}
        </div>
      </div>
      <aside style={{ width: "var(--panel-width)", flex: "0 0 auto", display: "flex", flexDirection: "column", gap: "var(--space-5)" }}>
        <span className="hb-meta">steps · ION-118</span>
        <Card padding="var(--space-5)" style={{ display: "flex", flexDirection: "column", gap: "var(--space-5)" }}>
          {steps.map((s, i) => (
            <Checkbox key={s.t} checked={s.done} label={s.t}
              onChange={() => setSteps((a) => a.map((x, j) => j === i ? { ...x, done: !x.done } : x))} />
          ))}
          <Button variant="secondary" size="sm" iconLeft="plus" fullWidth>Add a step</Button>
        </Card>
        <Card padding="var(--space-5)" style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
          <span className="hb-meta">notes</span>
          <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
            Steps are 2–5 minute physical actions. They live on an action's checklist and never become actions themselves.
          </p>
        </Card>
      </aside>
    </div>
  );
}
Object.assign(window, { RouteScreen });
