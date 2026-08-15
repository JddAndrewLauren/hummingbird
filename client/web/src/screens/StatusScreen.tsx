import { demoQuestions } from "../fixtures/demo";
import type { Screen } from "../shell/screens";
import type { CalendarReadDTO } from "../store/protocol";
import type { TaskState } from "../store/store";
import { SingleColumn } from "./layout";
import { realQuestionInputs } from "./NowScreen";
import { RankedRegion } from "./questions/RankedRegion";

// ADR-0017's Status screen (#311): the second surface of the same ranked
// region ADR-0015 designed for `NowScreen`'s aside — a board, not a bespoke
// tile grid, so collapse-when-dormant gives it "all green is one quiet
// stack, red announces itself" for free (ADR-0017 decision 1).
//
// Thin by design: nothing decidable is decided in this file. The surface
// filter, the sort and the collapse map are pure modules beside it today,
// and ADR-0025 moves that kind of decision further out still — into the
// core, reached through `src/decisions/seam.ts` — as each screen is built
// for Android. Neither move touches this component, which is the point; this
// component's only job is to pick the surface's inputs — real or demo — and
// hand them to the same `RankedRegion` `NowScreen` uses, which is what keeps
// this a second surface of one contract rather than a parallel screen that
// can drift from it.

export interface StatusScreenProps {
  onScreen: (screen: Screen) => void;
  task: TaskState;
  /** The one clock this screen gets — `App.tsx`'s `useSyncWiring` tick,
   * threaded straight through exactly as `NowScreen` does. */
  nowMs: number;
  calendarReads: Record<string, CalendarReadDTO | undefined>;
  calendarConnected: boolean;
}

export function StatusScreen({
  onScreen,
  task,
  nowMs,
  calendarReads,
  calendarConnected,
}: StatusScreenProps) {
  return (
    <SingleColumn>
      <RankedRegion
        surface="status"
        inputs={
          demoQuestions(nowMs) ?? realQuestionInputs(task, calendarReads, calendarConnected)
        }
        nowMs={nowMs}
        syncOutcomeSeq={task.syncOutcomeSeq}
        storage={typeof localStorage === "undefined" ? undefined : localStorage}
        onScreen={onScreen}
      />
    </SingleColumn>
  );
}
