import {
  pollerAnswerFromCore,
  pollerBandFromCore,
  pollerFactsFromCore,
  pollerSubjectsFromCore,
  type PaneInputsSource,
  type PollerFacts as PollerFactsCore,
  type PollerGap,
} from "../../decisions/seam";
import type { FreshnessDTO } from "../../store/protocol";
import type { Band, PaneAnswer, PaneGlyph, QuestionInputs } from "../questions/contract";

// **The poller-health question** (#775), answered over #245's pane shell —
// the first pane in this family with no browser-only predecessor.
//
// Every rule this file could hold instead lives in
// `hummingbird_core::decisions::panes::poller`: the watched-source list, the
// band (freshness alone, no body parser at all), and the gap kind. Read that
// module for the reasoning behind any of them.
//
// What is here is what ADR-0025 leaves per-client: the words and the glyph.

/** Pinned against `poller_constants_json()`'s own `sources` by
 * `seam.test.ts` — `question.ts` builds `sources: SOURCES` at module
 * evaluation, `github.ts`'s own arrangement (that module's own comment on
 * why these stay literal TS rather than a wasm call at import time). */
export const SOURCES: readonly string[] = [
  "gmail/v1",
  "m365-mail/v1",
  "google-calendar/v1",
  "m365-calendar/v1",
  "city-waste/v2",
  "race-schedule/v1",
  "kimi-balance/v1",
  "github-hummingbird/v1",
  "uptime/v1",
];

export const OVERDUE_MULTIPLIER = 3;
export const FLOOR_MS = 10 * 60 * 1000;

export interface PollerView {
  source: string;
  freshness: FreshnessDTO;
  band: Band;
}

function paneInputs(inputs: QuestionInputs): PaneInputsSource {
  return { nowMs: inputs.nowMs, bindings: inputs.bindings, paneReads: inputs.paneReads };
}

/** This question's subjects — `poller.rs`'s `poller_subjects`: every
 * watched source, always. */
export function pollerSubjects(inputs: QuestionInputs): string[] {
  return pollerSubjectsFromCore(paneInputs(inputs));
}

/** This source's band, from its freshest row's freshness alone —
 * `poller.rs`'s `poller_band`. */
export function pollerBand(freshness: FreshnessDTO): Band {
  return pollerBandFromCore(freshness);
}

function toView(source: string, facts: PollerFactsCore): PollerView {
  return { source, freshness: facts.freshness, band: facts.band };
}

/** The whole answered view, or `null` when there is nothing to answer with
 * (this source has never been read on this device). */
export function pollerView(source: string, inputs: QuestionInputs): PollerView | null {
  const resolved = pollerFactsFromCore(source, paneInputs(inputs));
  return resolved.kind === "facts" ? toView(source, resolved) : null;
}

const UNRESOLVABLE = "No answer yet.";

function gapReason(gap: PollerGap): string {
  switch (gap.gap) {
    case "notFetched":
      return "No answer has been fetched yet.";
    default:
      return UNRESOLVABLE;
  }
}

/** Why this pane has no answer, in words — read only when [`pollerView`]
 * returned `null`. */
export function pollerGapReason(source: string, inputs: QuestionInputs): string {
  const resolved = pollerFactsFromCore(source, paneInputs(inputs));
  return resolved.kind === "gap" ? gapReason(resolved.gap) : UNRESOLVABLE;
}

function ageWords(ageMs: number): string {
  const hours = Math.floor(ageMs / 3_600_000);
  if (hours < 1) {
    const minutes = Math.floor(ageMs / 60_000);
    return minutes < 1 ? "under a minute ago" : `${minutes}m ago`;
  }
  if (hours < 48) {
    return `${hours}h ago`;
  }
  return `${Math.floor(hours / 24)}d ago`;
}

/** The collapsed row's whole sentence, naming the source. */
export function pollerCollapsedHeadline(view: PollerView): string {
  if (view.freshness.kind === "unknown") {
    return `${view.source} · age unknown`;
  }
  const heardAgo = ageWords(view.freshness.ageMs);
  switch (view.band) {
    case "imminent":
      return `${view.source} · overdue, last row ${heardAgo}`;
    case "distant":
      return `${view.source} · cadence unreadable, last row ${heardAgo}`;
    default:
      return `${view.source} · healthy, last row ${heardAgo}`;
  }
}

/** One glyph naming the band. */
export function pollerGlyph(view: PollerView): PaneGlyph {
  if (view.band === "imminent") {
    return { kind: "icon", name: "siren", label: `${view.source} overdue` };
  }
  if (view.band === "distant") {
    return { kind: "icon", name: "help-circle", label: `${view.source} cadence unreadable` };
  }
  return { kind: "icon", name: "circle-check", label: `${view.source} healthy` };
}

/** This question's answer for the shell (#775).
 *
 * **The gap headline names the source**, unlike `github.ts`/`uptime.ts`'s
 * bare "No answer yet": those panes have exactly one never-polled sentinel
 * subject at a time, so a bare sentence never collides. Here every one of
 * `poller.rs`'s watched sources is a subject from the start, so a fresh
 * device shows several gap tiles at once — a bare "No answer yet" on all of
 * them would give the board (and a screen reader) several tiles with the
 * identical accessible name. */
export function pollerAnswer(source: string, inputs: QuestionInputs): PaneAnswer {
  const answer = pollerAnswerFromCore(source, paneInputs(inputs));
  const view = pollerView(source, inputs);
  if (view === null) {
    return {
      ...answer,
      collapsedHeadline: `${source} · No answer yet`,
      icon: [{ kind: "icon", name: "cloud-fog", label: `${source} no answer yet` }],
    };
  }

  return {
    ...answer,
    collapsedHeadline: pollerCollapsedHeadline(view),
    icon: [pollerGlyph(view)],
  };
}

