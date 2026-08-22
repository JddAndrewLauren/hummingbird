import {
  githubAnswerFromCore,
  githubBandFromCore,
  githubFactsFromCore,
  githubObservedAtMsFromCore,
  githubSubjectsFromCore,
  parseWorkflowBodyFromCore,
  type PaneInputsSource,
  type WorkflowFacts as WorkflowFactsCore,
  type WorkflowGap,
} from "../../decisions/seam";
import type { FreshnessDTO, PaneSnapshotDTO } from "../../store/protocol";
import type { Band, PaneAnswer, PaneGlyph, QuestionInputs } from "../questions/contract";
import { isStaleFreshness } from "../questions/freshness";

// **The GitHub workflow-health question** (#314, ADR-0017 decision 2),
// answered over #245's pane shell — and since #534, **the web's rendering
// half of it only**.
//
// Every rule this file used to hold is now
// `hummingbird_core::decisions::panes::github`: the payload parser, the
// subjects list, the observation instant the band is judged against, the
// band itself (including the stale-poller escalation), and the gap kinds.
// Read that module for the reasoning behind any of them.
//
// What stayed here is what ADR-0025 leaves per-client: **the words and the
// glyph**. `githubCollapsedHeadline`, `githubGlyph`, `ageWords` and
// `githubGapReason`.

/** These five constants stay literal TS, pinned against `github_constants_json()`
 * by `seam.test.ts` — `question.ts` builds `sources: [SOURCE]` at module
 * evaluation, exactly `waste.ts`'s own arrangement. */
export const SOURCE = "github-hummingbird/v1";
export const NEVER_POLLED_SUBJECT = "pending";
export const STALE_AFTER_MS = 6 * 60 * 60 * 1000;
export const OVERDUE_MULTIPLIER = 3;
export const MIN_OVERDUE_AFTER_MS = 3 * 60 * 60 * 1000;

/** The `github-hummingbird/v1` payload body — the shape is pinned by
 * `github.rs`'s `parse_workflow_body`; this is its wire form. */
export interface WorkflowBody {
  displayName: string;
  declaredCadenceMs: number | null;
  lastRunConclusion: string | null;
  lastRunEvent: string | null;
  lastRunAtMs: number | null;
  lastScheduledSuccessAtMs: number | null;
}

/** A body that could be read, or the reason it could not. */
export type WorkflowParse = { kind: "ok"; body: WorkflowBody } | { kind: "gap"; reason: string };

/** Reads one snapshot row into a body, or says why it could not —
 * `github.rs`'s `parse_workflow_body` with this client's wording put back
 * on. */
export function parseWorkflowBody(snapshot: PaneSnapshotDTO | undefined): WorkflowParse {
  const parsed = parseWorkflowBodyFromCore(snapshot);
  return parsed.kind === "ok" ? { kind: "ok", body: parsed.body } : { kind: "gap", reason: gapReason(parsed.gap) };
}

/** Re-exported from the shell for the same reason `waste.ts` re-exports it. */
export { isStaleFreshness };

/** When the poller observed what this row reports — `github.rs`'s
 * `observed_at_ms`, which is the instant every band call below judges
 * against. `null` when the row's freshness cannot locate it. */
export function observedAtMs(nowMs: number, freshness: FreshnessDTO): number | null {
  return githubObservedAtMsFromCore(nowMs, freshness);
}

/** This workflow's band, at read time — `github.rs`'s `github_band`.
 *
 * `observedAtMs` is when the *poller* looked, not when this reader is
 * looking; passing `nowMs` here is the bug that made a healthy
 * every-15-minutes workflow read "cron stalled" for all but the first
 * 45min after each poll. Build it with [`observedAtMs`]. */
export function githubBand(body: WorkflowBody, observedAtMs: number | null): Band {
  return githubBandFromCore(body, observedAtMs);
}

function ageWords(ageMs: number): string {
  const hours = Math.floor(ageMs / 3_600_000);
  if (hours < 1) {
    return "under an hour ago";
  }
  if (hours < 48) {
    return `${hours}h ago`;
  }
  return `${Math.floor(hours / 24)}d ago`;
}

/** The collapsed row's whole sentence, naming the workflow. */
export function githubCollapsedHeadline(body: WorkflowBody, nowMs: number, observedAtMs: number | null): string {
  const band = githubBand(body, observedAtMs);
  switch (band) {
    case "live":
      return `${body.displayName} · never run`;
    case "imminent":
      return body.lastScheduledSuccessAtMs === null
        ? `${body.displayName} · no scheduled success`
        : `${body.displayName} · stalled, last ok ${ageWords(nowMs - body.lastScheduledSuccessAtMs)}`;
    case "near":
      return `${body.displayName} · last run failed`;
    case "distant": {
      const lastSuccessAtMs = body.lastScheduledSuccessAtMs ?? nowMs;
      return `${body.displayName} · cadence unreadable, last scheduled success ${ageWords(nowMs - lastSuccessAtMs)}`;
    }
    default:
      return `${body.displayName} · healthy`;
  }
}

/** One glyph naming the band. */
export function githubGlyph(body: WorkflowBody, observedAtMs: number | null): PaneGlyph {
  const band = githubBand(body, observedAtMs);
  if (band === "live" || band === "imminent") {
    return { kind: "icon", name: "siren", label: `${body.displayName} ${band === "live" ? "never run" : "stalled"}` };
  }
  if (band === "near") {
    return { kind: "icon", name: "bell", label: `${body.displayName} last run failed` };
  }
  if (band === "distant") {
    return { kind: "icon", name: "help-circle", label: `${body.displayName} cadence unreadable` };
  }
  return { kind: "icon", name: "circle-check", label: `${body.displayName} healthy` };
}

/** Everything one answered pane needs, read by both the answer and the
 * expanded rendering. */
export interface WorkflowView {
  fileName: string;
  body: WorkflowBody;
  stale: boolean;
  freshness: FreshnessDTO;
}

function paneInputs(inputs: QuestionInputs): PaneInputsSource {
  return { nowMs: inputs.nowMs, bindings: inputs.bindings, paneReads: inputs.paneReads };
}

/** This question's subjects — `github.rs`'s `github_subjects`. */
export function githubSubjects(inputs: QuestionInputs): string[] {
  return githubSubjectsFromCore(paneInputs(inputs));
}

function toView(fileName: string, facts: WorkflowFactsCore): WorkflowView {
  return { fileName, body: facts.body, stale: facts.stale, freshness: facts.freshness };
}

/** The whole answered view, or `null` when there is nothing to answer with
 * (never polled, a payload that could not be read). */
export function githubView(fileName: string, inputs: QuestionInputs): WorkflowView | null {
  const resolved = githubFactsFromCore(fileName, paneInputs(inputs));
  return resolved.kind === "view" ? toView(fileName, resolved) : null;
}

/** Why this pane has no answer, in words — read only when [`githubView`]
 * returned `null`. */
export function githubGapReason(fileName: string, inputs: QuestionInputs): string {
  const resolved = githubFactsFromCore(fileName, paneInputs(inputs));
  return resolved.kind === "gap" ? gapReason(resolved.gap) : "No answer yet.";
}

const UNRESOLVABLE = "No answer yet.";

function gapReason(gap: WorkflowGap): string {
  switch (gap.gap) {
    case "notFetched":
      return "No answer has been fetched yet.";
    case "malformed":
      return `The workflow payload couldn't be read: ${gap.reason}`;
    case "unknownSchema":
      return `This device doesn't know how to read ${gap.schema} yet. Update the app.`;
    case "notJson":
      return "The workflow payload isn't JSON.";
    case "notAnObject":
      return "The workflow payload isn't an object.";
    case "unreadableFields":
      return "The workflow payload's fields can't be read.";
    default:
      return UNRESOLVABLE;
  }
}

/** This question's answer for the shell (#314 over #245/ADR-0017). */
export function githubAnswer(subjectKey: string, inputs: QuestionInputs): PaneAnswer {
  if (subjectKey === NEVER_POLLED_SUBJECT) {
    return {
      answerState: "bound-but-unacquired",
      band: "dormant",
      withinBand: null,
      collapsedHeadline: "No answer yet",
      icon: [{ kind: "icon", name: "cloud-fog", label: "no answer yet" }],
    };
  }

  const source = paneInputs(inputs);
  const answer = githubAnswerFromCore(subjectKey, source);
  const view = githubView(subjectKey, inputs);
  if (view === null) {
    return {
      ...answer,
      collapsedHeadline: "No answer yet",
      icon: [{ kind: "icon", name: "cloud-fog", label: "no answer yet" }],
    };
  }

  // The core's own stale-escalation already decided `answer.band`; the
  // words just have to agree with it, exactly as `waste.ts`'s answer does.
  // Escalation only ever lifts a `dormant`/`distant` raw band to
  // `imminent` — comparing the raw band (recomputed here, purely) against
  // the decided one is what tells the two "genuinely imminent" and
  // "escalated because stale" cases apart, since only `answer.band` and
  // `view.stale` alone cannot.
  const observedAt = observedAtMs(inputs.nowMs, view.freshness);
  const rawBand = githubBand(view.body, observedAt);
  if (view.stale && (rawBand === "dormant" || rawBand === "distant")) {
    const heardAgo = view.freshness.kind === "age" ? ageWords(view.freshness.ageMs) : "an unknown time ago";
    return {
      ...answer,
      collapsedHeadline: `${view.body.displayName} · answer may be stale, last heard ${heardAgo}`,
      icon: [{ kind: "icon", name: "cloud-fog", label: `${view.body.displayName} answer may be stale` }],
    };
  }

  return {
    ...answer,
    collapsedHeadline: githubCollapsedHeadline(view.body, inputs.nowMs, observedAt),
    icon: [githubGlyph(view.body, observedAt)],
  };
}
