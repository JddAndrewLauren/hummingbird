import { useCallback, useEffect, useRef, useState } from "react";
import { CLOUD_RUNNER_ENTRY } from "../skills/backend-registry";
import { grillRunBody, type GrillTurn } from "../skills/grill-args";
import { IDLE, reduceGrillTurn, type GrillTurnState } from "../skills/grill-turn-state";
import { runSkill, type RunSkillDeps } from "../skills/run-skill";
import { createIndexedDbTaskTokenStore, type TaskTokenStoreLike } from "../task/token-store";

// The Grill takeover's wiring (#355, ADR-0023): one typed turn at a time,
// on the main thread, exactly `useMicrotaskWiring.ts`'s reasoning —
// re-stated briefly here, not re-argued: a Grill turn is a question, and
// questions go stale (#269), so it stays outside the sync queue and the
// SharedWorker, with an `AbortController` (never a timer) the only way a
// turn ends early. `skills/no-queue.test.ts`'s `LANE_MODULES` covers this
// file for exactly that invariant.
//
// **Deliberately no #274 routing.** `useMicrotaskWiring.ts` picks a
// backend through `route-run.ts`'s Auto-fallback because a checklist run is
// worth retrying against a second tier; #355's brief asks for no comforts
// beyond the interview itself, and a backend picker is one — this hook
// always runs against the one registered entry (`CLOUD_RUNNER_ENTRY`)
// through the bare `runSkill` seam, with no picker and no fallback offer.
//
// **Confirming a proposal is not this hook's job.** The interview and the
// Confirm mutation are two different lanes by design: this file only ever
// asks the runner a question, and the composite `Core::complete_grill`
// write (`store/worker-client.ts`'s `completeGrill`) is an ordinary queued
// mutation, wired the same way `useTriageWiring.ts` wires `Core::triage` —
// see that hook, not this one, for Confirm.

export interface GrillWiring {
  /** The open item's turn state, or `IDLE`. Keyed by item so a turn started
   * on one item never renders under another (`useMicrotaskWiring.ts`'s
   * `run`, same per-item map). */
  turn: GrillTurnState;
  /** Every completed round for the open item, in order — what a caller
   * threads back as the next request's `turns`, and what the review card
   * formats into `Core::complete_grill`'s `transcript`
   * (`skills/grill-args.ts`'s `formatGrillTranscript`). */
  turns: GrillTurn[];
  /** Opens the interview: sends an empty `turns` array. A no-op while a
   * turn for this item is already asking — the same duplicate-tap rule
   * `useMicrotaskWiring.ts` documents. */
  onAsk: (itemId: string, ref: string) => void;
  /** Answers the open question: appends `{question, answer}` to the
   * item's own turns and asks again. */
  onAnswer: (itemId: string, ref: string, answer: string) => void;
  /** "Keep grilling", from a rendered proposal: asks again with the SAME
   * turns — the proposal was terminal, not a round to append, so there is
   * nothing to add before asking once more. A deliberate, user-clicked
   * re-request, never an automatic retry. */
  onKeepGrilling: (itemId: string, ref: string) => void;
  /** Discards whatever turn state an item holds — Back, or a fresh open
   * over a different item. Aborts an in-flight request first, so a stale
   * response can never land after the takeover has moved on. */
  onDiscard: (itemId: string) => void;
}

export interface GrillWiringDeps {
  fetch?: typeof globalThis.fetch;
  tokenStore?: TaskTokenStoreLike;
}

export function useGrillWiring(selectedItemId: string | null, deps: GrillWiringDeps = {}): GrillWiring {
  const [turnsByItem, setTurnsByItem] = useState<Record<string, GrillTurnState>>({});
  const [historyByItem, setHistoryByItem] = useState<Record<string, GrillTurn[]>>({});
  // One controller per item, and the in-flight lock — exactly
  // `useMicrotaskWiring.ts`'s `inFlight` ref, same reasoning: a shared
  // controller would let starting a turn on item B abort item A's.
  const inFlight = useRef<Map<string, AbortController>>(new Map());

  useEffect(() => {
    const running = inFlight.current;
    return () => {
      for (const controller of running.values()) controller.abort();
      running.clear();
    };
  }, []);

  const ask = useCallback(
    (itemId: string, ref: string, turns: GrillTurn[]) => {
      if (inFlight.current.has(itemId)) return;
      const controller = new AbortController();
      inFlight.current.set(itemId, controller);

      const runDeps: RunSkillDeps = {
        fetch: deps.fetch ?? globalThis.fetch.bind(globalThis),
        readToken: async () => (await store(deps.tokenStore).read())?.token ?? null,
        signal: controller.signal,
      };

      void (async () => {
        let state: GrillTurnState = IDLE;
        try {
          for await (const event of runSkill(CLOUD_RUNNER_ENTRY, grillRunBody({ ref, turns }), runDeps)) {
            state = reduceGrillTurn(state, event);
            const next = state;
            setTurnsByItem((current) => ({ ...current, [itemId]: next }));
          }
        } finally {
          inFlight.current.delete(itemId);
        }
      })();
    },
    [deps.fetch, deps.tokenStore],
  );

  const onAsk = useCallback(
    (itemId: string, ref: string) => {
      setHistoryByItem((current) => ({ ...current, [itemId]: [] }));
      ask(itemId, ref, []);
    },
    [ask],
  );

  const onAnswer = useCallback(
    (itemId: string, ref: string, answer: string) => {
      const state = turnsByItem[itemId] ?? IDLE;
      if (state.phase !== "question") return;
      const nextTurns = [...(historyByItem[itemId] ?? []), { question: state.question, answer }];
      setHistoryByItem((current) => ({ ...current, [itemId]: nextTurns }));
      ask(itemId, ref, nextTurns);
    },
    [ask, turnsByItem, historyByItem],
  );

  const onKeepGrilling = useCallback(
    (itemId: string, ref: string) => {
      const state = turnsByItem[itemId] ?? IDLE;
      if (state.phase !== "proposal") return;
      ask(itemId, ref, historyByItem[itemId] ?? []);
    },
    [ask, turnsByItem, historyByItem],
  );

  const onDiscard = useCallback((itemId: string) => {
    inFlight.current.get(itemId)?.abort();
    inFlight.current.delete(itemId);
    setTurnsByItem((current) => {
      if (!(itemId in current)) return current;
      const next = { ...current };
      delete next[itemId];
      return next;
    });
    setHistoryByItem((current) => {
      if (!(itemId in current)) return current;
      const next = { ...current };
      delete next[itemId];
      return next;
    });
  }, []);

  const turn = (selectedItemId && turnsByItem[selectedItemId]) || IDLE;
  const turns = (selectedItemId && historyByItem[selectedItemId]) || [];

  return { turn, turns, onAsk, onAnswer, onKeepGrilling, onDiscard };
}

let cachedStore: TaskTokenStoreLike | null = null;
function store(injected?: TaskTokenStoreLike): TaskTokenStoreLike {
  if (injected) return injected;
  cachedStore ??= createIndexedDbTaskTokenStore();
  return cachedStore;
}
