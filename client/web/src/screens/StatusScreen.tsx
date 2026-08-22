import type { CalendarReadDTO } from "../store/protocol";
import type { TaskState } from "../store/store";
import { SingleColumn } from "./layout";
import { realQuestionInputs } from "./NowScreen";
import { StatusBoard } from "./status-board/StatusBoard";

// ADR-0017's Status screen (#311), redrawn as the design handoff's board of
// expanding tiles.
//
// Still thin by design: nothing decidable is decided in this file. What
// changed is only how the decided panes are *drawn* — this screen no longer
// instantiates the same `RankedRegion` `NowScreen`'s aside uses, because a
// board and an aside now want different chrome for the same answers. The
// panes, their bands, their sentences and their glyphs are the identical
// `rankPanes(…, "status")` output either way; `status-board/StatusBoard.tsx`
// carries the reasoning for what it does and does not reuse from the region.
//
// It takes no `onScreen` any more. The region threaded one so an *unbound*
// pane could offer its setup prompt, and no question on this surface has a
// per-device binding to be unbound from: the four answer `answered` or
// `bound-but-unacquired` and say so in words. A prompt that can never render
// is a prop that can never fire.

export interface StatusScreenProps {
  task: TaskState;
  /** Whether this device can currently reach the authority — the sync
   * strip's own input, the same value the header's sync pill reads. */
  online: boolean;
  /** The one clock this screen gets — `App.tsx`'s `useSyncWiring` tick,
   * threaded straight through exactly as `NowScreen` does. */
  nowMs: number;
  calendarReads: Record<string, CalendarReadDTO | undefined>;
  calendarConnected: boolean;
}

export function StatusScreen({
  task,
  online,
  nowMs,
  calendarReads,
  calendarConnected,
}: StatusScreenProps) {
  return (
    <SingleColumn>
      <StatusBoard
        inputs={realQuestionInputs(task, calendarReads, calendarConnected)}
        nowMs={nowMs}
        online={online}
        queueDepth={task.queueDepth}
        lastSyncOutcome={task.lastSyncOutcome}
        lastSyncAtMs={task.lastSyncAtMs}
        storage={typeof localStorage === "undefined" ? undefined : localStorage}
      />
    </SingleColumn>
  );
}
