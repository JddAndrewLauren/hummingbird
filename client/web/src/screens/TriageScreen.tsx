import { useState } from "react";
import { Button } from "../components/core/Button";
import { Card } from "../components/core/Card";
import { Icon } from "../components/core/Icon";
import { StageBadge } from "../components/domain/StageBadge";
import { EmptyState } from "../components/feedback/EmptyState";
import { Input } from "../components/forms/Input";
import { Select } from "../components/forms/Select";
import { Slider } from "../components/forms/Slider";
import type { DemoCapture, DemoData } from "../fixtures/demo";
import { SingleColumn } from "./layout";

const CONTEXTS = ["@home", "@computer", "@phone", "@errands", "@garden", "@waiting"];

interface CaptureMeta {
  energy: number | null;
  size: number | null;
  context: string;
}

const EMPTY_META: CaptureMeta = { energy: null, size: null, context: "" };

export interface TriageScreenProps {
  demo: DemoData | null;
}

export function TriageScreen({ demo }: TriageScreenProps) {
  const [queue, setQueue] = useState<DemoCapture[]>(demo?.triage ?? []);
  const [draft, setDraft] = useState("");
  const [meta, setMeta] = useState<CaptureMeta>(EMPTY_META);

  // Capture writes nothing yet: there is no store to write to until S12
  // (#110) lands the real capture path. In demo mode the box works on the
  // fixture queue so the surface can be seen; in the real app it is inert
  // and says so rather than swallowing what someone typed.
  const canSubmit = demo !== null && draft.trim().length > 0;

  function submit() {
    if (!canSubmit) {
      return;
    }
    setQueue((current) => [
      { id: `CAP-${current.length + 8}`, title: draft, source: "Typed here", age: "just now" },
      ...current,
    ]);
    setDraft("");
    setMeta(EMPTY_META);
  }

  function drop(id: string) {
    setQueue((current) => current.filter((capture) => capture.id !== id));
  }

  return (
    <SingleColumn>
      <Card
        padding="var(--space-6)"
        style={{ display: "flex", flexDirection: "column", gap: "var(--space-6)" }}
      >
        <div style={{ display: "flex", alignItems: "flex-end", gap: "var(--space-5)", flexWrap: "wrap" }}>
          <Input
            style={{ flex: 1, minWidth: 260 }}
            label="Capture"
            icon="feather"
            placeholder="What's on your mind?"
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
          />
          <Button size="md" iconLeft="plus" disabled={!canSubmit} onClick={submit}>
            Add to Triage
          </Button>
        </div>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(3, 1fr)",
            gap: "var(--space-7)",
            alignItems: "start",
          }}
        >
          <Slider
            label="Energy"
            options={["low", "medium", "high"]}
            value={meta.energy}
            onChange={(energy) => setMeta({ ...meta, energy })}
          />
          <Slider
            label="Size"
            options={["quick", "normal", "deep"]}
            value={meta.size}
            onChange={(size) => setMeta({ ...meta, size })}
          />
          <Select
            label="Context"
            value={meta.context}
            onChange={(event) => setMeta({ ...meta, context: event.target.value })}
            options={[
              { value: "", label: "Not set" },
              ...CONTEXTS.map((context) => ({ value: context, label: context })),
            ]}
          />
        </div>
        <span className="hb-meta">
          {demo
            ? "optional — stage, dates and everything else are decided at mint time"
            : "capture is not wired to the core yet — nothing typed here is stored"}
        </span>
      </Card>

      <div>
        <div
          style={{
            display: "flex",
            alignItems: "baseline",
            justifyContent: "space-between",
            marginBottom: "var(--space-4)",
          }}
        >
          <h3 style={{ font: "var(--type-h3)", color: "var(--text-primary)" }}>Triage</h3>
          <span className="hb-meta">{queue.length} unsorted · swept every 15m</span>
        </div>
        {queue.length === 0 ? (
          <Card padding="0">
            <EmptyState
              icon="inbox"
              title="Triage is empty"
              body="Everything captured has been sorted. The sweeper drains again in 15 minutes."
            />
          </Card>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
            {queue.map((capture) => (
              <Card
                key={capture.id}
                padding="var(--space-5)"
                style={{ display: "flex", alignItems: "center", gap: "var(--space-5)" }}
              >
                <StageBadge stage="triage" />
                <span style={{ flex: 1, minWidth: 0, font: "var(--type-body)", color: "var(--text-primary)" }}>
                  {capture.title}
                </span>
                <span className="hb-meta">
                  {capture.source} · {capture.age}
                </span>
                <div
                  style={{
                    display: "flex",
                    gap: "var(--space-3)",
                    flexWrap: "wrap",
                    justifyContent: "flex-end",
                  }}
                >
                  <Button size="sm" variant="quiet" iconLeft="sparkles" onClick={() => drop(capture.id)}>
                    Mint action
                  </Button>
                  <Button
                    size="sm"
                    variant="secondary"
                    iconLeft="help-circle"
                    onClick={() => drop(capture.id)}
                  >
                    Grill
                  </Button>
                  <Button size="sm" variant="ghost" iconLeft="x" onClick={() => drop(capture.id)}>
                    Drop
                  </Button>
                </div>
              </Card>
            ))}
          </div>
        )}
      </div>

      <Card
        padding="var(--space-5)"
        style={{
          display: "flex",
          alignItems: "center",
          gap: "var(--space-5)",
          background: "var(--surface-quiet)",
        }}
      >
        <Icon name="info" size={16} color="var(--text-muted)" />
        <span style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
          Captures are created here first, then acked in their source. A capture source is drained; a
          context source never is.
        </span>
      </Card>
    </SingleColumn>
  );
}
