import { useCallback, useRef, useState } from "react";
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
//
// **`sessionSteps` is a snapshot, captured ONCE per session, never a live
// read.** #354's `Core::complete_grill` compares the review's own
// `session_steps` against the item's LIVE Steps at confirm time
// (`unticked_steps_changed`) to force a re-review when they drift — a
// guard that can only ever fire if the snapshot it compares against is
// actually frozen at some point before confirm. Reading `TaskState
// .stepsByItem` fresh on every render (what an earlier version of this
// hook did, re-requesting on every `syncOutcomeSeq` tick) defeats that on
// arrival: the "snapshot" would always equal whatever is live the instant
// Confirm is pressed, so the guard could never observe a difference. So
// this hook takes the CALLER's live `stepsByItem` only to notice the one
// moment a fresh answer lands after `open()` asks for it, copies that one
// answer into `sessionSteps` state, and never reads `stepsByItem` again
// for the rest of the session — `back()` is the only thing that clears it,
// exactly the way it clears the turn lane's own state.
export interface GrillTakeoverWiring {
  /** The item the takeover is open over, or `null` when it is closed —
   * Triage renders its ordinary list exactly when this is `null`. */
  openItemId: string | null;
  /** This session's frozen snapshot of the open item's Steps — `null`
   * until the one `getSteps` answer this session asked for lands (a brief
   * window right after `open()`), and unaffected by any later Steps
   * change until the NEXT session (`open()`) takes a fresh one. The
   * review card's plan-stranding check and `confirm`'s own
   * `session_steps` both read this, never `TaskState.stepsByItem`
   * directly. */
  sessionSteps: StepDTO[] | null;
  /** Opens the takeover over this item, starts the interview, and asks
   * for a fresh Steps snapshot. */
  open: (itemId: string) => void;
  /** Back: closes the takeover, discarding whatever turn state and Steps
   * snapshot it held (aborting an in-flight request first). Restoring
   * focus to the originating row is the screen's own job, not this
   * hook's. */
  back: () => void;
  turn: GrillTurnState;
  turns: GrillTurn[];
  answer: (text: string) => void;
  keepGrilling: () => void;
  retry: () => void;
  /** Confirms against THIS session's `sessionSteps` — never a fresh read.
   * A no-op while the snapshot has not landed yet, or while a previous
   * Confirm for this same session is already in flight (the synchronous
   * lock a double click needs — `Core::complete_grill` mints a brand-new
   * Grill id per call, so two enqueued calls would mint two Grills). */
  confirm: (completion: GrillCompletion) => void;
}

export interface GrillTakeoverWiringDeps {
  fetch?: typeof globalThis.fetch;
  tokenStore?: TaskTokenStoreLike;
}

export function useGrillTakeoverWiring(
  worker: WorkerLike,
  /** `TaskState.stepsByItem` — read here only to notice the one fresh
   * answer each session waits for; never re-read afterward. */
  stepsByItem: Record<string, StepDTO[]>,
  deps: GrillTakeoverWiringDeps = {},
): GrillTakeoverWiring {
  const [openItemId, setOpenItemId] = useState<string | null>(null);
  const [sessionSteps, setSessionSteps] = useState<StepDTO[] | null>(null);
  // Whatever `stepsByItem` already held for this item the moment `open()`
  // was called — the value a fresh answer must differ from (by reference;
  // `worker-client.ts` always installs a new array on a real answer, so a
  // stale cache from some earlier, unrelated read can never be mistaken
  // for this session's own). `undefined` means "never read at all before
  // this session", which a first-ever answer already differs from.
  //
  // STATE, not a ref: `react-hooks/refs` forbids reading `ref.current`
  // during render (refs are for values outside the render cycle), and this
  // value is compared during render on purpose — the identical
  // "adjusting state when a prop changes" pattern
  // (`TriageRow.tsx`/`GrillTakeover.tsx`'s own precedent) applies to it
  // exactly as it does to `sessionSteps` itself.
  const [priorSteps, setPriorSteps] = useState<StepDTO[] | undefined>(undefined);
  const grillWiring = useGrillWiring(openItemId, deps);
  const { completeGrill } = useGrillCompletionWiring(worker);

  if (openItemId !== null && sessionSteps === null) {
    const current = stepsByItem[openItemId];
    if (current !== undefined && current !== priorSteps) {
      setSessionSteps(current);
    }
  }

  // The synchronous double-click lock `confirm` needs: `TaskState.pending`
  // only reflects a queued mutation once a later `isPending` read answers,
  // which is too late to stop a second click landing before the first
  // one's effects are visible. A plain ref, not state — the same shape
  // `useGrillWiring.ts`'s own `inFlight` uses for the identical reason.
  const confirming = useRef<Set<string>>(new Set());

  const open = useCallback(
    (itemId: string) => {
      setPriorSteps(stepsByItem[itemId]);
      setOpenItemId(itemId);
      setSessionSteps(null);
      confirming.current.delete(itemId);
      requestSteps(worker, itemId);
      grillWiring.onAsk(itemId, itemId);
    },
    // `stepsByItem` MUST stay a real dependency — `open` reads its
    // CURRENT value at click time (what a fresh answer must differ from),
    // and a stale closure here would compare against whatever was live
    // the first time this hook rendered, forever.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [worker, stepsByItem, grillWiring.onAsk],
  );

  const back = useCallback(() => {
    if (openItemId !== null) grillWiring.onDiscard(openItemId);
    setOpenItemId(null);
    setSessionSteps(null);
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

  const retry = useCallback(() => {
    if (openItemId !== null) grillWiring.onRetry(openItemId, openItemId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [openItemId, grillWiring.onRetry]);

  const confirm = useCallback(
    (completion: GrillCompletion) => {
      if (openItemId === null || sessionSteps === null) return;
      if (confirming.current.has(openItemId)) return;
      confirming.current.add(openItemId);
      completeGrill(openItemId, sessionSteps, completion);
      // Optimistic close, same posture as every other mutation in this
      // app: `Core::complete_grill`'s enqueue is synchronous and durable
      // (it does not wait on a network round trip to be real), so there is
      // nothing left for this takeover to hold open for — the item's new
      // stage shows up through the ordinary `triageInbox`/`frontier`
      // re-read the `completeGrillResult` broadcast already triggers.
      // Closing here is also what makes a double click physically
      // impossible a second way: the Confirm button this session owned is
      // gone once `openItemId` is `null`.
      grillWiring.onDiscard(openItemId);
      setOpenItemId(null);
      setSessionSteps(null);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [openItemId, sessionSteps, completeGrill, grillWiring.onDiscard],
  );

  return {
    openItemId,
    sessionSteps,
    open,
    back,
    turn: grillWiring.turn,
    turns: grillWiring.turns,
    answer,
    keepGrilling,
    retry,
    confirm,
  };
}
