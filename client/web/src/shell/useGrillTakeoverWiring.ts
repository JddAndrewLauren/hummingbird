import { useCallback, useEffect, useState } from "react";
import type { GrillTurn } from "../skills/grill-args";
import type { GrillTurnState } from "../skills/grill-turn-state";
import type { TaskTokenStoreLike } from "../task/token-store";
import type { StepDTO } from "../store/protocol";
import { requestSteps, type GrillCompletion, type WorkerLike } from "../store/worker-client";
import { useGrillCompletionWiring } from "./useGrillCompletionWiring";
import { useGrillWiring } from "./useGrillWiring";

// The Triage screen's own composition point (#355, ADR-0023): which item's
// takeover is open, the turn-asking lane (`useGrillWiring.ts`, kept pure
// per `skills/no-queue.test.ts`) and the Confirm mutation
// (`useGrillCompletionWiring.ts`) stitched into the one thing the screen
// actually renders against — the same role `App.tsx` plays for every other
// feature, scoped to this one screen because nothing outside Triage opens
// a Grill takeover in this slice.
export interface GrillTakeoverWiring {
  /** The item the takeover is open over, or `null` when it is closed —
   * Triage renders its ordinary list exactly when this is `null`. */
  openItemId: string | null;
  /** Opens the takeover over this item and starts the interview — the row
   * button's own click handler. */
  open: (itemId: string) => void;
  /** Back: closes the takeover, discarding whatever turn state it held
   * (aborting an in-flight request first). Restoring focus to the
   * originating row is the screen's own job, not this hook's. */
  back: () => void;
  turn: GrillTurnState;
  turns: GrillTurn[];
  answer: (text: string) => void;
  keepGrilling: () => void;
  confirm: (sessionSteps: StepDTO[], completion: GrillCompletion) => void;
}

export interface GrillTakeoverWiringDeps {
  fetch?: typeof globalThis.fetch;
  tokenStore?: TaskTokenStoreLike;
}

export function useGrillTakeoverWiring(
  worker: WorkerLike,
  syncOutcomeSeq: number,
  deps: GrillTakeoverWiringDeps = {},
): GrillTakeoverWiring {
  const [openItemId, setOpenItemId] = useState<string | null>(null);
  const grillWiring = useGrillWiring(openItemId, deps);
  const { completeGrill } = useGrillCompletionWiring(worker);

  // The review card's plan-stranding check reads this item's live Steps
  // (`screens/grill-review.ts`) — requested the moment the takeover opens,
  // same "ask the moment selection changes" contract
  // `useItemDetailWiring.ts` documents for its own Steps read, and
  // re-requested per sync cycle for the same reason: a Step ticked or added
  // elsewhere while the interview runs must not go stale.
  useEffect(() => {
    if (openItemId !== null) {
      requestSteps(worker, openItemId);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [openItemId, syncOutcomeSeq]);

  const open = useCallback(
    (itemId: string) => {
      setOpenItemId(itemId);
      grillWiring.onAsk(itemId, itemId);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [grillWiring.onAsk],
  );

  const back = useCallback(() => {
    if (openItemId !== null) grillWiring.onDiscard(openItemId);
    setOpenItemId(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [openItemId, grillWiring.onDiscard]);

  const answer = useCallback(
    (text: string) => {
      if (openItemId !== null) grillWiring.onAnswer(openItemId, openItemId, text);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [openItemId, grillWiring.onAnswer],
  );

  const keepGrilling = useCallback(() => {
    if (openItemId !== null) grillWiring.onKeepGrilling(openItemId, openItemId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [openItemId, grillWiring.onKeepGrilling]);

  const confirm = useCallback(
    (sessionSteps: StepDTO[], completion: GrillCompletion) => {
      if (openItemId !== null) completeGrill(openItemId, sessionSteps, completion);
    },
    [openItemId, completeGrill],
  );

  return { openItemId, open, back, turn: grillWiring.turn, turns: grillWiring.turns, answer, keepGrilling, confirm };
}
