import { relativeAge, syncOutcomeClass } from "../../shell/sync-status";
import { SYNC_TIMER_MS } from "../../shell/sync-cadence";
import type { PaneAnswer, QuestionInputs } from "../questions/contract";

export const SUBJECT_KEY = "reachability";
const MAX_SYNC_BACKOFF_MS = 5 * 60_000;
/** One ordinary cycle may fail before the core enters its maximum backoff.
 * Keep the pane quiet until both that cadence and the whole backoff ceiling
 * have elapsed, so it cannot announce failure before a retry is eligible. */
export const REACHABILITY_GRACE_MS = SYNC_TIMER_MS + MAX_SYNC_BACKOFF_MS;

export interface ReachabilityView {
  ageMs: number;
  headline: string;
  stale: boolean;
}

/** The client-only reachability decision. Its only clock is `inputs.nowMs`;
 * the persisted success is kept even when a newer attempt fails. */
export function reachabilityView(inputs: QuestionInputs): ReachabilityView | null {
  const lastSuccessfulAtMs = inputs.sync.lastSuccessfulAtMs;
  if (lastSuccessfulAtMs === null) {
    return null;
  }

  const ageMs = Math.max(0, inputs.nowMs - lastSuccessfulAtMs);
  const latestClass = inputs.sync.latestOutcome
    ? syncOutcomeClass(inputs.sync.latestOutcome.kind)
    : null;
  const latestAttemptLanded =
    latestClass === "landed" && inputs.sync.latestInformativeAtMs === lastSuccessfulAtMs;

  return {
    ageMs,
    headline: `${latestAttemptLanded ? "Synced" : "Last synced"} ${relativeAge(ageMs)}`,
    stale: ageMs > REACHABILITY_GRACE_MS,
  };
}

export function reachabilityAnswer(inputs: QuestionInputs): PaneAnswer {
  const lastSuccessfulAtMs = inputs.sync.lastSuccessfulAtMs;
  const view = reachabilityView(inputs);
  if (view === null || lastSuccessfulAtMs === null) {
    return {
      answerState: "bound-but-unacquired",
      band: "dormant",
      withinBand: null,
      collapsedHeadline: "Never synced on this device.",
      icon: [{ kind: "icon", name: "cloud-fog", label: "never synced on this device" }],
    };
  }

  return {
    answerState: "answered",
    band: view.stale ? "live" : "dormant",
    withinBand: lastSuccessfulAtMs + REACHABILITY_GRACE_MS,
    collapsedHeadline: view.headline,
    icon: [
      view.stale
        ? { kind: "icon", name: "siren", label: "authority sync stale" }
        : { kind: "icon", name: "circle-check", label: "authority recently reached" },
    ],
  };
}
