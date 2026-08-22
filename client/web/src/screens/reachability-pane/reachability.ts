import { relativeAge } from "../../shell/sync-status";
import {
  reachabilityAnswerFromCore,
  reachabilityFactsFromCore,
  type PaneInputsSource,
} from "../../decisions/seam";
import type { PaneAnswer, QuestionInputs } from "../questions/contract";

// **The client-side reachability question** (#316), answered over #245's
// pane shell — and since #534, **the web's rendering half of it only**.
//
// Every rule this file used to hold is now
// `hummingbird_core::decisions::panes::reachability`: the grace window
// (computed from `SYNC_TIMER_MS` + the sync core's own maximum backoff),
// the sync-outcome classification, and the band. Read that module for the
// reasoning behind any of them.
//
// What stayed here is the headline sentence: `relativeAge`'s wording, and
// the choice between "Synced" and "Last synced".

export const SUBJECT_KEY = "reachability";
/** Pinned against `reachability_constants_json()`'s `graceMs` by
 * `seam.test.ts` — the same module-evaluation constraint every other
 * pane's constants stay literal for. */
export const REACHABILITY_GRACE_MS = 60_000 + 5 * 60_000;

export interface ReachabilityView {
  ageMs: number;
  headline: string;
  stale: boolean;
}

function paneInputs(inputs: QuestionInputs): PaneInputsSource {
  return {
    nowMs: inputs.nowMs,
    bindings: inputs.bindings,
    paneReads: inputs.paneReads,
    sync: inputs.sync,
  };
}

/** The client-only reachability decision — `reachability.rs`'s
 * `reachability_facts` with this client's headline word put back on. */
export function reachabilityView(inputs: QuestionInputs): ReachabilityView | null {
  const facts = reachabilityFactsFromCore(paneInputs(inputs));
  if (facts === null) {
    return null;
  }
  return {
    ageMs: facts.ageMs,
    headline: `${facts.latestAttemptLanded ? "Synced" : "Last synced"} ${relativeAge(facts.ageMs)}`,
    stale: facts.stale,
  };
}

/** Whether this pane has anything to say *under* its headline.
 *
 * Alone among the status questions, it usually does not: "Synced 12m ago" is
 * the entire answer, and the other three carry facts beneath theirs (a
 * workflow's last run, a probe's observation, a balance's split). Only the
 * gap arm has a body — the sentence explaining why there is no answer.
 *
 * The Status board reads this to decide whether to offer a disclosure
 * control at all. A toggle that opens onto nothing is worse than no toggle:
 * it reads as a broken control rather than as a pane with nothing to add. */
export function reachabilityHasDetail(inputs: QuestionInputs): boolean {
  return reachabilityView(inputs) === null;
}

export function reachabilityAnswer(inputs: QuestionInputs): PaneAnswer {
  const view = reachabilityView(inputs);
  const answer = reachabilityAnswerFromCore(paneInputs(inputs));
  if (view === null) {
    return {
      ...answer,
      collapsedHeadline: "Never synced on this device.",
      icon: [{ kind: "icon", name: "cloud-fog", label: "never synced on this device" }],
    };
  }

  return {
    ...answer,
    collapsedHeadline: view.headline,
    icon: [
      view.stale
        ? { kind: "icon", name: "siren", label: "authority sync stale" }
        : { kind: "icon", name: "circle-check", label: "authority recently reached" },
    ],
  };
}
