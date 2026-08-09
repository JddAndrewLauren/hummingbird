import { Badge } from "../components/core/Badge";
import { Button } from "../components/core/Button";
import { Card } from "../components/core/Card";
import { Icon } from "../components/core/Icon";
import { ContextTile } from "../components/domain/ContextTile";
import { ItemRow } from "../components/domain/ItemRow";
import { StageBadge } from "../components/domain/StageBadge";
import { EmptyState } from "../components/feedback/EmptyState";
import type { CalendarTileProps } from "../calendar/tile-props";
import type { DemoData, DemoSnapshot } from "../fixtures/demo";
import type { Screen } from "../shell/screens";
import { Aside, Column, Section, TwoColumn } from "./layout";

function SnapshotTile({ snapshot }: { snapshot: DemoSnapshot }) {
  return (
    <Card
      padding="var(--space-5)"
      style={{ flex: 1, display: "flex", flexDirection: "column", gap: "var(--space-2)" }}
    >
      <span className="hb-meta">{snapshot.name}</span>
      <span style={{ font: "var(--weight-bold) 22px/1.1 var(--font-display)", color: "var(--text-primary)" }}>
        {snapshot.value}
      </span>
      <span className="hb-meta" style={{ color: "var(--text-muted)" }}>
        {snapshot.note}
      </span>
    </Card>
  );
}

export interface NowScreenProps {
  demo: DemoData | null;
  /** The real calendar context, or null on a device that never opted in. */
  tile: CalendarTileProps | null;
  onScreen: (screen: Screen) => void;
}

export function NowScreen({ demo, tile, onScreen }: NowScreenProps) {
  const top = demo?.items[1];
  const rest = demo ? demo.items.filter((item) => item.id !== top?.id && item.stage !== "done") : [];

  return (
    <TwoColumn>
      <Column>
        {demo && top ? (
          <>
            <div>
              <span className="hb-meta">top pick · thursday</span>
              <Card
                accent
                elevation={2}
                padding="var(--space-7)"
                style={{
                  marginTop: "var(--space-4)",
                  display: "flex",
                  flexDirection: "column",
                  gap: "var(--space-5)",
                }}
              >
                <div style={{ display: "flex", alignItems: "center", gap: "var(--space-4)" }}>
                  <StageBadge stage={top.stage} />
                  {top.size ? <Badge mono>size:{top.size}</Badge> : null}
                  <Badge mono tone="brand">
                    {top.id}
                  </Badge>
                </div>
                <h2 style={{ font: "var(--type-h1)", letterSpacing: "var(--tracking-heading)" }}>
                  {top.title}
                </h2>
                <p style={{ font: "var(--type-body)", color: "var(--text-secondary)" }}>
                  Started 40 minutes ago. Three of seven steps ticked; the next one is
                  &ldquo;regenerate the fixture set&rdquo;.
                </p>
                <div style={{ display: "flex", gap: "var(--space-4)", flexWrap: "wrap" }}>
                  <Button iconLeft="play">Resume</Button>
                  <Button variant="secondary" iconLeft="list-checks" onClick={() => onScreen("routes")}>
                    Steps ({top.steps})
                  </Button>
                  <Button variant="ghost" iconLeft="clock">
                    Later today
                  </Button>
                </div>
              </Card>
            </div>
            <Section title="Also startable" meta={`${rest.length} actions`}>
              <Card padding="var(--space-3)">
                {rest.map((item) => (
                  <ItemRow
                    key={item.id}
                    title={item.title}
                    stage={item.stage}
                    urgency={item.urgency}
                    due={item.due}
                    scheduled={item.scheduled}
                    size={item.size}
                    steps={item.steps}
                    blockedBy={item.blockedBy}
                    onClick={() => onScreen("routes")}
                  />
                ))}
              </Card>
            </Section>
          </>
        ) : (
          <Card padding="var(--space-3)">
            <EmptyState
              icon="zap"
              title="Nothing to start"
              body="No actions exist yet. Ranking begins once the first one is minted from Triage."
            />
          </Card>
        )}
      </Column>

      <Aside>
        <div>
          <span className="hb-meta">calendar context</span>
          <div
            style={{
              marginTop: "var(--space-4)",
              display: "flex",
              flexDirection: "column",
              gap: "var(--space-4)",
            }}
          >
            {demo ? (
              <>
                <ContextTile
                  kind="in_progress"
                  title="Design review"
                  timeLabel="9:30–10:00 AM"
                  href="#"
                  asOf="just now"
                />
                <ContextTile
                  kind="upcoming"
                  title="School pickup"
                  timeLabel="3:10–3:30 PM"
                  asOf="42m ago"
                  stale
                />
              </>
            ) : tile ? (
              <ContextTile
                kind={tile.kind}
                title={tile.title}
                timeLabel={tile.timeLabel}
                href={tile.href}
                asOf={tile.asOf}
                stale={tile.stale}
              />
            ) : (
              <Card padding="var(--space-3)">
                <EmptyState
                  compact
                  icon="calendar-clock"
                  title="No calendar connected"
                  body="Connect Google Calendar in Settings to see what's on now and next."
                />
              </Card>
            )}
          </div>
        </div>

        {demo ? (
          <>
            <div>
              <span className="hb-meta">standing questions</span>
              <Card
                padding="var(--space-5)"
                style={{
                  marginTop: "var(--space-4)",
                  display: "flex",
                  flexDirection: "column",
                  gap: "var(--space-4)",
                }}
              >
                {demo.standingQuestions.map((question) => (
                  <div key={question.q} style={{ display: "flex", gap: "var(--space-4)" }}>
                    <Icon
                      name="help-circle"
                      size={16}
                      color="var(--text-muted)"
                      style={{ marginTop: 2 }}
                    />
                    <div>
                      <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
                        {question.q}
                      </p>
                      <p style={{ font: "var(--type-body-strong)" }}>{question.a}</p>
                    </div>
                  </div>
                ))}
              </Card>
            </div>
            <div>
              <span className="hb-meta">context snapshots</span>
              <div
                style={{
                  marginTop: "var(--space-4)",
                  display: "flex",
                  gap: "var(--space-4)",
                  flexWrap: "wrap",
                }}
              >
                {demo.snapshots.map((snapshot) => (
                  <SnapshotTile key={snapshot.name} snapshot={snapshot} />
                ))}
              </div>
            </div>
          </>
        ) : null}
      </Aside>
    </TwoColumn>
  );
}
