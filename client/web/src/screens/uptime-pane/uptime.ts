import {
  parseUptimeBodyFromCore,
  uptimeAnswerFromCore,
  uptimeBandFromCore,
  uptimeFactsFromCore,
  uptimeSubjectsFromCore,
  type PaneInputsSource,
  type ProbeFacts as ProbeFactsCore,
  type ProbeGap,
} from "../../decisions/seam";
import type { FreshnessDTO, PaneSnapshotDTO } from "../../store/protocol";
import type { Band, PaneAnswer, PaneGlyph, QuestionInputs } from "../questions/contract";
import { isStaleFreshness } from "../questions/freshness";

// **The uptime question** (#315, ADR-0017 decisions 2/3/4/6), answered over
// #245's pane shell — and since #534, **the web's rendering half of it
// only**.
//
// Every rule this file used to hold is now
// `hummingbird_core::decisions::panes::uptime`: the payload parser
// (including the mutual-exclusion check), the subjects list, the band, and
// the gap kinds. Read that module for the reasoning behind any of them.
//
// What stayed here is what ADR-0025 leaves per-client: **the words and the
// glyph**. `uptimeCollapsedHeadline`, `uptimeGlyph`, `ageWords` and
// `uptimeGapReason`.

export const SOURCE = "uptime/v1";
export const NEVER_POLLED_SUBJECT = "pending";
export const STALE_AFTER_MS = 3 * 60 * 60 * 1000;

/** The `uptime/v1` payload body — the shape is pinned by `uptime.rs`'s
 * `parse_uptime_body`; this is its wire form. */
export interface ProbeBody {
  expected: "on" | "off";
  expectStatus: number;
  observedStatus: number | null;
  error: string | null;
}

export type ProbeParse = { kind: "ok"; body: ProbeBody } | { kind: "gap"; reason: string };

/** Reads one snapshot row into a body, or says why it could not —
 * `uptime.rs`'s `parse_uptime_body` with this client's wording put back
 * on. */
export function parseUptimeBody(snapshot: PaneSnapshotDTO | undefined): ProbeParse {
  const parsed = parseUptimeBodyFromCore(snapshot);
  return parsed.kind === "ok" ? { kind: "ok", body: parsed.body } : { kind: "gap", reason: gapReason(parsed.gap) };
}

export { isStaleFreshness };

/** This service's band, at read time — `uptime.rs`'s `uptime_band`. */
export function uptimeBand(body: ProbeBody): Band {
  return uptimeBandFromCore(body);
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

/** The collapsed row's whole sentence, naming the service. */
export function uptimeCollapsedHeadline(serviceId: string, body: ProbeBody): string {
  if (body.expected === "off") {
    return body.error !== null
      ? `${serviceId} · off, as expected`
      : `${serviceId} · reachable — expected off`;
  }
  if (body.error !== null) {
    return `${serviceId} · unreachable — ${body.error}`;
  }
  if (body.observedStatus !== body.expectStatus) {
    return `${serviceId} · unexpected status ${body.observedStatus} (wanted ${body.expectStatus})`;
  }
  return `${serviceId} · ${body.observedStatus} as expected`;
}

/** One glyph naming the band. */
export function uptimeGlyph(serviceId: string, body: ProbeBody): PaneGlyph {
  const band = uptimeBand(body);
  if (band === "live") {
    return { kind: "icon", name: "siren", label: `${serviceId} divergent` };
  }
  if (band === "near") {
    return { kind: "icon", name: "bell", label: `${serviceId} unexpected status` };
  }
  return { kind: "icon", name: "circle-check", label: `${serviceId} as expected` };
}

/** Everything one answered pane needs, read by both the answer and the
 * expanded rendering. */
export interface ProbeView {
  serviceId: string;
  body: ProbeBody;
  stale: boolean;
  freshness: FreshnessDTO;
}

function paneInputs(inputs: QuestionInputs): PaneInputsSource {
  return { nowMs: inputs.nowMs, bindings: inputs.bindings, paneReads: inputs.paneReads };
}

/** This question's subjects — `uptime.rs`'s `uptime_subjects`. */
export function uptimeSubjects(inputs: QuestionInputs): string[] {
  return uptimeSubjectsFromCore(paneInputs(inputs));
}

function toView(serviceId: string, facts: ProbeFactsCore): ProbeView {
  return { serviceId, body: facts.body, stale: facts.stale, freshness: facts.freshness };
}

/** The whole answered view, or `null` when there is nothing to answer with. */
export function uptimeView(serviceId: string, inputs: QuestionInputs): ProbeView | null {
  const resolved = uptimeFactsFromCore(serviceId, paneInputs(inputs));
  return resolved.kind === "facts" ? toView(serviceId, resolved) : null;
}

/** Why this pane has no answer, in words — read only when [`uptimeView`]
 * returned `null`. */
export function uptimeGapReason(serviceId: string, inputs: QuestionInputs): string {
  const resolved = uptimeFactsFromCore(serviceId, paneInputs(inputs));
  return resolved.kind === "gap" ? gapReason(resolved.gap) : "No answer yet.";
}

const UNRESOLVABLE = "No answer yet.";

function gapReason(gap: ProbeGap): string {
  switch (gap.gap) {
    case "notFetched":
      return "No answer has been fetched yet.";
    case "malformed":
      return `The probe payload couldn't be read: ${gap.reason}`;
    case "unknownSchema":
      return `This device doesn't know how to read ${gap.schema} yet. Update the app.`;
    case "notJson":
      return "The probe payload isn't JSON.";
    case "notAnObject":
      return "The probe payload isn't an object.";
    case "fieldsUnreadable":
      return "The probe payload's fields can't be read.";
    case "observationUnreadable":
      return "The probe payload's observation can't be read.";
    default:
      return UNRESOLVABLE;
  }
}

/** This question's answer for the shell (#315 over ADR-0017). */
export function uptimeAnswer(subjectKey: string, inputs: QuestionInputs): PaneAnswer {
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
  const answer = uptimeAnswerFromCore(subjectKey, source);
  const view = uptimeView(subjectKey, inputs);
  if (view === null) {
    return {
      ...answer,
      collapsedHeadline: "No answer yet",
      icon: [{ kind: "icon", name: "cloud-fog", label: "no answer yet" }],
    };
  }

  // Escalation only ever lifts a `dormant` raw band to `imminent` — compare
  // the raw band against the decided one, same reasoning as `github.ts`.
  const rawBand = uptimeBand(view.body);
  if (view.stale && rawBand === "dormant") {
    const heardAgo = view.freshness.kind === "age" ? ageWords(view.freshness.ageMs) : "an unknown time ago";
    return {
      ...answer,
      collapsedHeadline: `${view.serviceId} · answer may be stale, last heard ${heardAgo}`,
      icon: [{ kind: "icon", name: "cloud-fog", label: `${view.serviceId} answer may be stale` }],
    };
  }

  return {
    ...answer,
    collapsedHeadline: uptimeCollapsedHeadline(view.serviceId, view.body),
    icon: [uptimeGlyph(view.serviceId, view.body)],
  };
}
