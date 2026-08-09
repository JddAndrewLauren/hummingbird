import React from "react";
import { Icon } from "../core/Icon.jsx";
import { Button } from "../core/Button.jsx";

const TIERS = {
  urgent: { fg: "var(--status-danger-fg)", bg: "var(--status-danger-bg)", icon: "siren" },
  normal: { fg: "var(--status-info-fg)", bg: "var(--status-info-bg)", icon: "bell" },
};

export function AlertCard({ tier = "normal", source, title, detail, time, href, acked = false, onAck, style = {}, ...rest }) {
  const t = TIERS[tier] || TIERS.normal;
  return (
    <div style={{
      display: "flex", gap: "var(--space-5)", padding: "var(--space-6)",
      background: "var(--surface-card)", border: "1px solid var(--border-subtle)",
      borderRadius: "var(--radius-card)", boxShadow: "var(--shadow-1)",
      opacity: acked ? 0.55 : 1, ...style,
    }} {...rest}>
      <span style={{ display: "inline-flex", alignItems: "center", justifyContent: "center",
        width: 32, height: 32, flex: "0 0 auto", borderRadius: "var(--radius-md)",
        background: t.bg, color: t.fg }}>
        <Icon name={t.icon} size={17} />
      </span>
      <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)", flex: 1, minWidth: 0 }}>
        <span style={{ display: "flex", alignItems: "center", gap: "var(--space-4)", font: "var(--type-meta)",
          letterSpacing: "var(--tracking-meta)", textTransform: "uppercase", color: "var(--text-muted)" }}>
          <span style={{ color: t.fg }}>{tier}</span>{source}{time ? <span>· {time}</span> : null}
        </span>
        <p style={{ font: "var(--type-body-strong)", color: "var(--text-primary)" }}>{title}</p>
        {detail ? <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>{detail}</p> : null}
        <div style={{ display: "flex", gap: "var(--space-4)", marginTop: "var(--space-3)", flexWrap: "wrap" }}>
          {acked ? <span style={{ font: "var(--type-body-sm)", color: "var(--text-muted)" }}>Acked</span>
            : <Button size="sm" variant="secondary" iconLeft="check" onClick={onAck}>Ack</Button>}
          {href ? <Button size="sm" variant="ghost" iconRight="arrow-up-right" onClick={() => window.open(href, "_blank")}>Open source</Button> : null}
        </div>
      </div>
    </div>
  );
}
