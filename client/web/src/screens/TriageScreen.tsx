import { useEffect, useRef, useState } from "react";
import { Button } from "../components/core/Button";
import { Card } from "../components/core/Card";
import { Icon } from "../components/core/Icon";
import { StageBadge } from "../components/domain/StageBadge";
import { EmptyState } from "../components/feedback/EmptyState";
import { Input } from "../components/forms/Input";
import { Select } from "../components/forms/Select";
import { Slider } from "../components/forms/Slider";
import type { DemoCapture, DemoData } from "../fixtures/demo";
import { CAPTURE_INPUT_ID } from "../shell/capture-hotkey";
import type { TaskState } from "../store/store";
import { canSubmitCapture } from "./capture-validation";
import { orderTriage } from "./triage-order";
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
  /** S12's real triage inbox (issue #110), rendered whenever `demo` is
   * null. */
  task: TaskState;
  /** Enqueues a real capture — `shell/useCaptureWiring.ts`'s `submitCapture`.
   * Never called with an empty/whitespace-only draft: `canSubmitCapture`
   * gates the button this component renders before this is ever reached
   * (#110's "an empty capture is refused client-side"). */
  onSubmitCapture: (title: string, nowMs: number) => void;
  /** Bumped by `App.tsx`'s global capture hotkey (and its header Capture
   * button) to request focus on the capture input from anywhere in the
   * app — #110's "a global hotkey that focuses it". `0` (the initial
   * value) never focuses; only a later, distinct value does, so mounting
   * this screen by navigating a screen at a time never steals focus on its
   * own. */
  focusRequestId: number;
}

export function TriageScreen({ demo, task, onSubmitCapture, focusRequestId }: TriageScreenProps) {
  const [queue, setQueue] = useState<DemoCapture[]>(demo?.triage ?? []);
  const [draft, setDraft] = useState("");
  const [meta, setMeta] = useState<CaptureMeta>(EMPTY_META);
  const mounted = useRef(false);

  // Moves focus to the capture input whenever a focus request arrives
  // (including the one that arrives on first mount if this screen was
  // navigated to BY the hotkey/header button itself — `App.tsx` bumps the
  // id and switches screens in the same gesture, and this screen mounts
  // fresh in that case, so the effect below still fires). `mounted` skips
  // only the truly first render (screen already open, id still its initial
  // 0), the same guard `Header.tsx`'s own focus-on-navigate effect uses.
  useEffect(() => {
    if (!mounted.current) {
      mounted.current = true;
      if (focusRequestId === 0) {
        return;
      }
    }
    document.getElementById(CAPTURE_INPUT_ID)?.focus();
  }, [focusRequestId]);

  const canSubmit = canSubmitCapture(draft);

  function submit() {
    if (!canSubmit) {
      return;
    }
    if (demo) {
      setQueue((current) => [
        { id: `CAP-${current.length + 8}`, title: draft, source: "Typed here", age: "just now" },
        ...current,
      ]);
    } else {
      onSubmitCapture(draft, Date.now());
    }
    setDraft("");
    setMeta(EMPTY_META);
  }

  function drop(id: string) {
    setQueue((current) => current.filter((capture) => capture.id !== id));
  }

  const realTriage = demo ? [] : orderTriage(task.triageInbox);

  return (
    <SingleColumn>
      <Card
        padding="var(--space-6)"
        style={{ display: "flex", flexDirection: "column", gap: "var(--space-6)" }}
      >
        <div style={{ display: "flex", alignItems: "flex-end", gap: "var(--space-5)", flexWrap: "wrap" }}>
          <Input
            id={CAPTURE_INPUT_ID}
            style={{ flex: 1, minWidth: 260 }}
            label="Capture"
            icon="feather"
            placeholder="What's on your mind?"
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              // `isComposing` guards an IME composition commit (e.g. an
              // Enter that confirms a candidate while typing Japanese/
              // Chinese/Korean) from being read as "submit" — that Enter
              // belongs to the composition, not to this form.
              if (event.key === "Enter" && !event.nativeEvent.isComposing) {
                submit();
              }
            }}
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
            : "optional — stage, dates and everything else are decided at mint time (not yet stored on a real capture)"}
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
          <h2 style={{ font: "var(--type-h3)", color: "var(--text-primary)" }}>Triage</h2>
          {/* The sweeper is off pending the authority move, and nothing in this
              app drains the queue — only demo mode may claim a cadence. */}
          <span className="hb-meta">
            {demo
              ? `${queue.length} unsorted · swept every 15m`
              : `${realTriage.length} unsorted`}
          </span>
        </div>
        {demo ? (
          queue.length === 0 ? (
            <Card padding="0">
              <EmptyState
                icon="inbox"
                headingLevel={3}
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
          )
        ) : realTriage.length === 0 ? (
          <Card padding="0">
            <EmptyState
              icon="inbox"
              headingLevel={3}
              title="Triage is empty"
              body="Nothing captured is waiting to be sorted."
            />
          </Card>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
            {realTriage.map((item) => (
              <Card
                key={item.id}
                padding="var(--space-5)"
                style={{ display: "flex", alignItems: "center", gap: "var(--space-5)" }}
              >
                <StageBadge stage="triage" />
                <span style={{ flex: 1, minWidth: 0, font: "var(--type-body)", color: "var(--text-primary)" }}>
                  {item.title}
                </span>
                {item.pending ? (
                  <span
                    title="Not yet confirmed by the server"
                    className="hb-meta"
                    style={{ display: "inline-flex", alignItems: "center", gap: "var(--space-2)" }}
                  >
                    <Icon name="loader-circle" size={13} />
                    Pending
                  </span>
                ) : null}
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
