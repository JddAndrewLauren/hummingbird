import { useState } from "react";
import { Badge } from "../components/core/Badge";
import { Card } from "../components/core/Card";
import { AlertCard } from "../components/domain/AlertCard";
import { EmptyState } from "../components/feedback/EmptyState";
import { Switch } from "../components/forms/Switch";
import type { DemoData } from "../fixtures/demo";
import { Aside, Column, TwoColumn } from "./layout";

export interface AlertsScreenProps {
  demo: DemoData | null;
}

export function AlertsScreen({ demo }: AlertsScreenProps) {
  const [acked, setAcked] = useState<Record<string, boolean>>({});
  const alerts = demo?.alerts ?? [];
  const live = alerts.filter((alert) => !acked[alert.id]);

  return (
    <TwoColumn>
      <Column>
        <div style={{ display: "flex", alignItems: "baseline", justifyContent: "space-between" }}>
          <h2 style={{ font: "var(--type-h3)", color: "var(--text-primary)" }}>Live</h2>
          <span className="hb-meta">{live.length} unacked · default-deny</span>
        </div>
        {live.length === 0 ? (
          <Card padding="0">
            <EmptyState
              icon="bell-off"
              title="Nothing is ringing"
              body={
                demo
                  ? "Every alert has been acked. What no rule matches stays silent."
                  : "No rules are wired to the notification lane yet. What no rule matches stays silent."
              }
            />
          </Card>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-5)" }}>
            {live.map((alert) => (
              <AlertCard
                key={alert.id}
                tier={alert.tier}
                source={alert.source}
                title={alert.title}
                detail={alert.detail}
                time={alert.time}
                href="#"
                acked={Boolean(acked[alert.id])}
                onAck={() => setAcked((current) => ({ ...current, [alert.id]: true }))}
              />
            ))}
          </div>
        )}
      </Column>

      {demo ? (
        <Aside>
          <span className="hb-meta">rules</span>
          {demo.rules.map((rule) => (
            <Card
              key={rule.name}
              padding="var(--space-5)"
              style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}
            >
              <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
                <span style={{ font: "var(--type-body-strong)", color: "var(--text-primary)" }}>
                  {rule.name}
                </span>
                <Badge mono tone={rule.tier === "urgent" ? "danger" : "info"}>
                  {rule.tier}
                </Badge>
              </div>
              <span style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
                {rule.description}
              </span>
            </Card>
          ))}
          <Card padding="var(--space-5)">
            <Switch
              checked
              label="Urgent tier only on the watch"
              hint="Normal-tier deliveries stay on phone and desktop."
            />
          </Card>
        </Aside>
      ) : null}
    </TwoColumn>
  );
}
