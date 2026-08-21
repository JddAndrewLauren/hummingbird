import { useEffect, useRef, useState } from "react";
import { Card } from "../components/core/Card";
import { Icon } from "../components/core/Icon";
import { EmptyState } from "../components/feedback/EmptyState";
import type { GrillTakeoverWiring } from "../shell/useGrillTakeoverWiring";
import type { TaskState } from "../store/store";
import type { TriageEdits } from "../store/worker-client";
import { GrillTakeover } from "./GrillTakeover";
import { grillCompletionFailureFor } from "./write-failure";
import { grillMeButtonId, TriageRow } from "./TriageRow";
import { triageProcessQueue } from "./triage-process-order";
import { SingleColumn } from "./layout";

export interface TriageScreenProps {
  /** S12's real triage inbox (issue #110). */
  task: TaskState;
  /** S13/#111's triage mutation — `shell/useTriageWiring.ts`'s `triage`.
   * Edits whatever `edits` sets and promotes the item to `destination`, as
   * one call. Optional so a render with no worker behind it never has to
   * pass a real one. */
  onTriage?: (
    itemId: string,
    destination: "ready" | null,
    edits: TriageEdits,
  ) => void;
  /** The row checkmark's `Core::act` complete — see `TriageRow`'s own prop
   * doc. */
  onComplete?: (itemId: string) => void;
  /** #631: each row's inline "new project" affordance — `TriageRow`'s own
   * prop doc carries the rest. Optional for the same "no worker, no
   * affordance" reason `onTriage` is. */
  onCreateProject?: (name: string) => void;
  /** "Now", for the age each collapsed row states. Passed in rather than read
   * here: `useSyncWiring`'s tick is the one clock this origin gets (ADR-0007),
   * and a screen that read `Date.now()` per render would be a second one. */
  nowMs: number;
  /** "Grill me" (#355, ADR-0023): `shell/useGrillTakeoverWiring.ts`'s whole
   * composite — which item (if any) has the takeover open, the turn state
   * and the Confirm mutation. */
  grill?: GrillTakeoverWiring;
}

// The Triage screen: the unsorted inbox, one line per capture, expanding into
// the full editor for whichever row is selected. The capture box itself is NOT
// here — it lives in the shell's `CapturePopover`, opened by the header's New
// button or the global hotkey from any screen, so this screen is where captures
// are *sorted*, never where they are typed.
export function TriageScreen({
  task,
  onTriage,
  onComplete,
  onCreateProject,
  nowMs,
  grill,
}: TriageScreenProps) {
  // One row open at a time: expanding is a *selection*, and two open editors
  // would put two sets of unsent drafts on screen with nothing to say which is
  // being worked. `null` is the resting state — an inbox is for reading first.
  const [selectedId, setSelectedId] = useState<string | null>(null);

  // #357: the "triage process" queue — local drafts, then Grilling-stage
  // items, then captured Triage items — as one combined, ordered read.
  // Neither this screen nor Now's collapsible area filters by stage on its
  // own; `triageProcessQueue` is the one function both render from.
  const triageQueue = triageProcessQueue(task.triageInbox, task.grillingItems, task.grillDraftItemIds);
  const realTriage = triageQueue.items;

  // Back restores focus to the row's own "Grill me" button — never a held
  // DOM reference, unlike `shell/CapturePopover.tsx`'s `restoreTo`: the
  // takeover unmounts the whole list, so the button that opened it is gone
  // by the time Back is pressed, and the one Back remounts is a NEW
  // element. Looked up by id instead (`TriageRow.tsx`'s `grillMeButtonId`,
  // the same "id survives the tree, a ref does not" contract
  // `CAPTURE_TRIGGER_ID` uses for its own trigger), the moment the takeover
  // actually closes.
  const focusOnCloseRef = useRef<string | null>(null);

  function handleGrillMe(itemId: string): void {
    focusOnCloseRef.current = itemId;
    grill?.open(itemId);
  }

  function handleGrillBack(): void {
    grill?.back();
  }

  useEffect(() => {
    if (grill?.openItemId !== null && grill?.openItemId !== undefined) {
      return;
    }
    const itemId = focusOnCloseRef.current;
    if (itemId === null) {
      return;
    }
    focusOnCloseRef.current = null;
    document.getElementById(grillMeButtonId(itemId))?.focus();
  }, [grill?.openItemId]);

  // #357: `realTriage` is the combined queue (Triage AND Grilling), so the
  // takeover's item must resolve against it too — resolving against
  // `task.triageInbox` alone would leave a Grilling row's "Grill me"/"Resume
  // grill" opening a takeover with no item to render, and no reachable
  // Back/Discard.
  const openItem = grill?.openItemId
    ? realTriage.find((item) => item.id === grill.openItemId)
    : undefined;

  if (grill && openItem) {
    return (
      <SingleColumn>
        <GrillTakeover
          item={openItem}
          steps={grill.sessionSteps}
          turn={grill.turn}
          turns={grill.turns}
          backLabel="Back to Triage"
          onAnswer={grill.answer}
          onKeepGrilling={grill.keepGrilling}
          onRetry={grill.retry}
          onConfirm={grill.confirm}
          onBack={handleGrillBack}
          onDiscard={grill.discard}
          completionError={grillCompletionFailureFor(task.lastGrillCompletion, grill.confirmSeed)}
        />
      </SingleColumn>
    );
  }

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
          <span className="hb-meta">
            {`${triageQueue.capturedCount} captured · ${triageQueue.grillingCount} grilling`}
          </span>
        </div>
        {realTriage.length === 0 ? (
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
                projects={task.projects ?? []}
                expanded={selectedId === item.id}
                onToggle={() => setSelectedId(selectedId === item.id ? null : item.id)}
                nowMs={nowMs}
                onTriage={onTriage}
                onComplete={onComplete}
                onGrillMe={grill ? handleGrillMe : undefined}
                hasGrillDraft={task.grillDraftItemIds.includes(item.id)}
                lastTriage={task.lastTriage}
                onCreateProject={onCreateProject}
                lastProjectWrite={task.lastProjectWrite}
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
