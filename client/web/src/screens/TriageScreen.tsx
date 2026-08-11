import { useState } from "react";
import { Button } from "../components/core/Button";
import { Card } from "../components/core/Card";
import { Icon } from "../components/core/Icon";
import { MarkDoneButton } from "../components/domain/MarkDoneButton";
import { StageBadge } from "../components/domain/StageBadge";
import { EmptyState } from "../components/feedback/EmptyState";
import type { DemoCapture, DemoData } from "../fixtures/demo";
import type { TriageDestinationName } from "../store/protocol";
import type { TaskState } from "../store/store";
import type { TriageEdits } from "../store/worker-client";
import { TriageRow } from "./TriageRow";
import { orderTriage } from "./triage-order";
import { SingleColumn } from "./layout";

export interface TriageScreenProps {
  demo: DemoData | null;
  /** S12's real triage inbox (issue #110), rendered whenever `demo` is
   * null. */
  task: TaskState;
  /** Demo mode's own unsorted list, and the drop that removes one from it.
   * Held by `App.tsx` rather than here since the capture box moved into the
   * shell's popover: a capture typed there has to land in the same list this
   * screen renders, and there is no path from a popover in the shell to state
   * private to a screen. Both absent outside demo mode, where the real
   * inbox (`task.triageInbox`) is what renders. */
  demoCaptures?: DemoCapture[];
  onDropDemoCapture?: (id: string) => void;
  /** S13/#111's triage mutation — `shell/useTriageWiring.ts`'s `triage`.
   * Edits whatever `edits` sets and promotes the item to `destination`, as
   * one call. Optional so a demo-only render (no worker behind it) never
   * has to pass a real one. */
  onTriage?: (itemId: string, destination: TriageDestinationName, edits: TriageEdits) => void;
  /** The row checkmark's `Core::act` complete — see `TriageRow`'s own prop
   * doc. Optional for the same demo-only reason as `onTriage`. */
  onComplete?: (itemId: string) => void;
  /** "Now", for the age each collapsed row states. Passed in rather than read
   * here: `useSyncWiring`'s tick is the one clock this origin gets (ADR-0007),
   * and a screen that read `Date.now()` per render would be a second one. */
  nowMs: number;
}

// The Triage screen: the unsorted inbox, one line per capture, expanding into
// the full editor for whichever row is selected. The capture box itself is NOT
// here — it lives in the shell's `CapturePopover`, opened by the header's New
// button or the global hotkey from any screen, so this screen is where captures
// are *sorted*, never where they are typed.
export function TriageScreen({
  demo,
  task,
  demoCaptures,
  onDropDemoCapture,
  onTriage,
  onComplete,
  nowMs,
}: TriageScreenProps) {
  const queue = demoCaptures ?? [];
  // One row open at a time: expanding is a *selection*, and two open editors
  // would put two sets of unsent drafts on screen with nothing to say which is
  // being worked. `null` is the resting state — an inbox is for reading first.
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const realTriage = demo ? [] : orderTriage(task.triageInbox);

  return (
    <SingleColumn>
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
                  style={{ display: "flex", alignItems: "center", gap: "var(--space-5)", flexWrap: "wrap" }}
                >
                  <StageBadge stage="triage" />
                  {/* Found by the G2 visual gate at 768 (`docs/SURFACES.md`):
                      with a plain `flex: 1, minWidth: 0` and no overflow
                      rule, the meta and the button strip take the whole row
                      and this span is squeezed to a few pixels — its text
                      wrapped one word per line and rendered straight through
                      the meta beside it. The `220px` basis is a floor, not a
                      width: the row wraps the meta and buttons onto their own
                      line before the title is starved, and the ellipsis (the
                      same contract `ItemRow` uses) handles what is still too
                      long after that. */}
                  <span style={{ flex: "1 1 220px", minWidth: 0, font: "var(--type-body)", color: "var(--text-primary)",
                    overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                    {capture.title}
                  </span>
                  <span className="hb-meta" style={{ flex: "0 0 auto", whiteSpace: "nowrap" }}>
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
                    <Button size="sm" variant="quiet" iconLeft="sparkles" onClick={() => onDropDemoCapture?.(capture.id)}>
                      Mint action
                    </Button>
                    <Button
                      size="sm"
                      variant="secondary"
                      iconLeft="help-circle"
                      onClick={() => onDropDemoCapture?.(capture.id)}
                    >
                      Grill
                    </Button>
                    <Button size="sm" variant="ghost" iconLeft="x" onClick={() => onDropDemoCapture?.(capture.id)}>
                      Drop
                    </Button>
                    {/* Demo's mark-done checkmark drops the capture, the same
                        demo-local resolution every other button on this row
                        uses — so `?demo` photographs the real shell. */}
                    <MarkDoneButton
                      title={capture.title}
                      onClick={() => onDropDemoCapture?.(capture.id)}
                    />
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
              <TriageRow
                key={item.id}
                item={item}
                projects={task.projects}
                expanded={selectedId === item.id}
                onToggle={() => setSelectedId(selectedId === item.id ? null : item.id)}
                nowMs={nowMs}
                onTriage={onTriage}
                onComplete={onComplete}
              />
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
