const { Card, Button, Badge, Input, EmptyState, Icon, StageBadge, Slider, Select } = window.HummingbirdDesignSystem_dcdceb;

const HB_CONTEXTS = ["@home","@computer","@phone","@errands","@garden","@waiting"];

// Optional capture metadata: two sliders + a context dropdown. All optional —
// unset is the default, deciding is mint-time work.
function CaptureMeta({ meta, onMeta, columns = 3 }) {
  return (
    <div style={{ display: "grid", gridTemplateColumns: `repeat(${columns}, 1fr)`, gap: "var(--space-7)", alignItems: "start" }}>
      <Slider label="Energy" options={["low", "medium", "high"]} value={meta.energy}
        onChange={(v) => onMeta({ ...meta, energy: v })} />
      <Slider label="Size" options={["quick", "normal", "deep"]} value={meta.size}
        onChange={(v) => onMeta({ ...meta, size: v })} />
      <Select label="Context" value={meta.context} onChange={(e) => onMeta({ ...meta, context: e.target.value })}
        options={[{ value: "", label: "Not set" }, ...HB_CONTEXTS.map((c) => ({ value: c, label: c }))]} />
    </div>
  );
}
Object.assign(window, { CaptureMeta, HB_CONTEXTS });

function TriageScreen({ data, onDrain }) {
  const [queue, setQueue] = React.useState(data.triage);
  const [draft, setDraft] = React.useState("");
  const [meta, setMeta] = React.useState({ energy: null, size: null, context: "" });
  const take = (id, stage) => setQueue((q) => q.filter((c) => c.id !== id));
  return (
    <div style={{ maxWidth: "var(--content-max)", display: "flex", flexDirection: "column", gap: "var(--space-7)" }}>
      <Card padding="var(--space-6)" style={{ display: "flex", flexDirection: "column", gap: "var(--space-6)" }}>
        <div style={{ display: "flex", alignItems: "flex-end", gap: "var(--space-5)", flexWrap: "wrap" }}>
          <Input style={{ flex: 1, minWidth: 260 }} label="Capture" icon="feather" placeholder="What's on your mind?"
            value={draft} onChange={(e) => setDraft(e.target.value)} />
          <Button size="md" iconLeft="plus" onClick={() => { if (draft.trim()) { setQueue((q) => [{ id: "CAP-" + (q.length + 8), title: draft, source: "Typed here", age: "just now" }, ...q]); setDraft(""); setMeta({ energy: null, size: null, context: "" }); } }}>Add to Triage</Button>
        </div>
        <CaptureMeta meta={meta} onMeta={setMeta} />
        <span className="hb-meta">optional — stage, dates and everything else are decided at mint time</span>
      </Card>
      <div>
        <div style={{ display: "flex", alignItems: "baseline", justifyContent: "space-between", marginBottom: "var(--space-4)" }}>
          <h3 style={{ font: "var(--type-h3)" }}>Triage</h3>
          <span className="hb-meta">{queue.length} unsorted · swept every 15m</span>
        </div>
        {queue.length === 0 ? (
          <Card padding="0"><EmptyState icon="inbox" title="Triage is empty"
            body="Everything captured has been sorted. The sweeper drains again in 15 minutes." /></Card>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
            {queue.map((c) => (
              <Card key={c.id} padding="var(--space-5)" style={{ display: "flex", alignItems: "center", gap: "var(--space-5)" }}>
                <StageBadge stage="triage" />
                <span style={{ flex: 1, minWidth: 0, font: "var(--type-body)" }}>{c.title}</span>
                <span className="hb-meta">{c.source} · {c.age}</span>
                <div style={{ display: "flex", gap: "var(--space-3)", flexWrap: "wrap", justifyContent: "flex-end" }}>
                  <Button size="sm" variant="quiet" iconLeft="sparkles" onClick={() => take(c.id, "ready")}>Mint action</Button>
                  <Button size="sm" variant="secondary" iconLeft="help-circle" onClick={() => take(c.id, "grilling")}>Grill</Button>
                  <Button size="sm" variant="ghost" iconLeft="x" onClick={() => take(c.id)}>Drop</Button>
                </div>
              </Card>
            ))}
          </div>
        )}
      </div>
      <Card padding="var(--space-5)" style={{ display: "flex", alignItems: "center", gap: "var(--space-5)", background: "var(--surface-quiet)" }}>
        <Icon name="info" size={16} color="var(--text-muted)" />
        <span style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
          Captures are created here first, then acked in their source. A capture source is drained; a context source never is.
        </span>
      </Card>
    </div>
  );
}
Object.assign(window, { TriageScreen });
