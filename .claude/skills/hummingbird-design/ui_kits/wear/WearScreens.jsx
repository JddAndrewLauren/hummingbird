const { Icon, Badge, StageBadge } = window.HummingbirdDesignSystem_dcdceb;

function Watch({ children, label }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 12 }}>
      <div style={{ width: 240, height: 240, borderRadius: "50%", background: "#000", padding: 10,
        boxShadow: "0 24px 48px -18px rgba(0,0,0,.5), 0 0 0 6px #1a1d20, 0 0 0 7px #33383d" }}>
        <div data-theme="dark" style={{ width: "100%", height: "100%", borderRadius: "50%", overflow: "hidden",
          background: "var(--surface-page)", color: "var(--text-primary)", display: "flex", flexDirection: "column",
          alignItems: "center", justifyContent: "center", padding: "26px 22px", textAlign: "center", gap: 8 }}>
          {children}
        </div>
      </div>
      <span className="hb-meta">{label}</span>
    </div>
  );
}

function WearNext() {
  return (
    <Watch label="Next up">
      <span className="hb-meta" style={{ color: "var(--text-brand)" }}>next up</span>
      <p style={{ font: "var(--weight-bold) 16px/1.25 var(--font-display)", color: "var(--text-primary)" }}>Order the replacement sensor</p>
      <div style={{ display: "flex", gap: 6, justifyContent: "center" }}><StageBadge stage="ready" /></div>
      <span className="hb-meta">due fri · size:quick</span>
    </Watch>
  );
}

function WearAlert() {
  return (
    <Watch label="Urgent alert · ack">
      <span style={{ display: "inline-flex", alignItems: "center", justifyContent: "center", width: 34, height: 34,
        borderRadius: "50%", background: "var(--status-danger-bg)", color: "var(--status-danger-fg)" }}>
        <Icon name="siren" size={18} />
      </span>
      <p style={{ font: "var(--weight-bold) 15px/1.25 var(--font-display)", color: "var(--text-primary)" }}>Sweeper run failed</p>
      <span className="hb-meta">fly · hb-worker · 6m</span>
      <button style={{ marginTop: 4, height: 34, padding: "0 20px", borderRadius: 999, border: "none",
        background: "var(--accent)", color: "#fff", font: "var(--weight-semibold) 14px/1 var(--font-sans)" }}>Ack</button>
    </Watch>
  );
}

function WearContext() {
  return (
    <Watch label="Calendar context">
      <span className="hb-meta" style={{ color: "var(--text-brand)" }}>now</span>
      <p style={{ font: "var(--weight-bold) 16px/1.25 var(--font-display)", color: "var(--text-primary)" }}>Design review</p>
      <span style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>9:30–10:00 AM</span>
      <span className="hb-meta" style={{ color: "var(--status-warn-fg)" }}>stale — as of 42m ago</span>
    </Watch>
  );
}
Object.assign(window, { WearNext, WearAlert, WearContext });
