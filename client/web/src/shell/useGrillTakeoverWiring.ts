import { useCallback, useEffect, useRef, useState } from "react";
import type { GrillTurn } from "../skills/grill-args";
import type { GrillTurnState } from "../skills/grill-turn-state";
import type { TaskTokenStoreLike } from "../task/token-store";
import type { StepDTO } from "../store/protocol";
import type { TaskGrillCompletionResult } from "../store/store";
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
//
// **Confirm closes the takeover on the authority's `"ok"`, never
// optimistically.** Every other mutation in this app closes its editor the
// moment it enqueues, because for those an enqueue really is the whole
// answer. `Core::complete_grill` is the exception: it can *refuse to
// enqueue at all*, synchronously (`client/core/src/lib.rs` —
// `NeedsReReview` when the item's unticked Steps drifted from this
// session's snapshot, plus `ItemNotFound`/`ItemDone`), and #354's whole
// reason for that guard is to send the reader BACK to this review with the
// drift in mind. An optimistic close would tear the takeover, the turn
// state and the transcript down before that refusal arrived, so the guard
// would fire into an empty room: the item silently stays in Triage, the
// transcript it wanted re-reviewed is gone, and nothing on screen says so.
// So the takeover stays up until the matching `completeGrillResult` says
// `"ok"`, and a non-ok answer releases the `confirming` lock and leaves the
// review card — transcript, edits and all — standing to be re-reviewed.
// The double-click line is held by `confirming` (synchronous, below) and
// the button's own `disabled`, not by the unmount.
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
   * Grill id per call, so two enqueued calls would mint two Grills).
   *
   * Does NOT close the takeover: see this module's own note on why the
   * close waits for an `"ok"`. */
  confirm: (completion: GrillCompletion) => void;
  /** The `seed` of the Confirm this session currently has outstanding, or
   * `null` when it has none. The screen scopes the review card's error to
   * it (`write-failure.ts`'s `grillCompletionFailureFor`) so a failure only
   * ever speaks for the confirm that produced it. */
  confirmSeed: string | null;
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
  /** `TaskState.lastGrillCompletion` — the broadcast this hook waits on to
   * decide whether a Confirm may close the takeover. */
  lastGrillCompletion: TaskGrillCompletionResult | null,
  deps: GrillTakeoverWiringDeps = {},
): GrillTakeoverWiring {
  const [openItemId, setOpenItemId] = useState<string | null>(null);
  const [sessionSteps, setSessionSteps] = useState<StepDTO[] | null>(null);
  const [confirmSeed, setConfirmSeed] = useState<string | null>(null);
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
      // A fresh session owes nothing to the last one's confirm: dropping the
      // seed here is what stops a previous session's failure — still sitting
      // in `lastGrillCompletion`, which only ever holds the most recent
      // result — from being rendered against this session's new proposal,
      // which it does not describe.
      setConfirmSeed(null);
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
    if (openItemId !== null) {
      grillWiring.onDiscard(openItemId);
      confirming.current.delete(openItemId);
    }
    setOpenItemId(null);
    setSessionSteps(null);
    // Leaving deliberately abandons whatever confirm was outstanding: a
    // later answer for it has no review card left to speak to, and keeping
    // the seed would only arm the stale error the next session opens with.
    setConfirmSeed(null);
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
    if (openItemId === null) return;
    grillWiring.onKeepGrilling(openItemId, openItemId);
    // Another round supersedes the proposal a failed confirm was made
    // against, so its error stops describing anything on screen — the same
    // reseeding the review card does to its own editable fields when a new
    // proposal arrives.
    setConfirmSeed(null);
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
      // The takeover stays up until this seed's own answer says `"ok"` —
      // see the module note. Recording the seed is what makes that answer
      // recognisable as this confirm's, rather than "the most recent
      // completion result, whoever it belongs to".
      setConfirmSeed(completeGrill(openItemId, sessionSteps, completion));
    },
    [openItemId, sessionSteps, completeGrill],
  );

  // This session's confirm, answered — `null` until the broadcast carrying
  // THIS seed lands. `lastGrillCompletion` holds one slot for the whole app,
  // so the seed comparison is the whole of "is this mine".
  const confirmAnswer =
    confirmSeed !== null && lastGrillCompletion?.seed === confirmSeed ? lastGrillCompletion : null;

  // An `"ok"` is the only thing that closes the takeover. Every other kind
  // leaves it standing, so the review card renders the refusal over the very
  // transcript the reader now has to re-review — the module note above says
  // why that is the whole point.
  //
  // The same "adjusting state when a prop changes" pattern as `sessionSteps`
  // above (`NowScreen.tsx`'s own precedent, and its reasoning): a `setState`
  // during render, guarded against state, because `react-hooks/refs` forbids
  // touching a ref during render and `react-hooks/set-state-in-effect`
  // forbids the `useEffect` spelling of exactly this adjustment.
  if (confirmAnswer?.kind === "ok" && openItemId !== null) {
    setOpenItemId(null);
    setSessionSteps(null);
  }

  // The imperative half of the same answer, which a render may not do:
  // releasing the synchronous lock (a ref write) and discarding the turn
  // lane's state. `confirmSeed` deliberately survives BOTH outcomes here —
  // on a failure because it is what keeps the message on the card, and on an
  // `"ok"` because nothing renders it once the takeover is closed, while
  // clearing it during render would retract `confirmAnswer` before this
  // effect ever ran. `open`, `back` and `keepGrilling` drop it, each for its
  // own reason above.
  const discardOnDone = grillWiring.onDiscard;
  useEffect(() => {
    if (confirmAnswer === null) return;
    confirming.current.delete(confirmAnswer.itemId);
    if (confirmAnswer.kind === "ok") discardOnDone(confirmAnswer.itemId);
  }, [confirmAnswer, discardOnDone]);

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
    confirmSeed,
  };
}
