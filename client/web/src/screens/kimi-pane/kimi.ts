import {
  kimiAnswerFromCore,
  kimiBandFromCore,
  kimiFactsFromCore,
  parseKimiBodyFromCore,
  type KimiFacts as KimiFactsCore,
  type KimiGap,
  type PaneInputsSource,
} from "../../decisions/seam";
import type { FreshnessDTO, PaneSnapshotDTO } from "../../store/protocol";
import type { PaneAnswer, PaneGlyph, QuestionInputs } from "../questions/contract";
import { isStaleFreshness } from "../questions/freshness";

// **The Kimi balance question** (#313, ADR-0017 decision 5), answered over
// #245's pane shell — and since #534, **the web's rendering half of it
// only**.
//
// Every rule this file used to hold is now
// `hummingbird_core::decisions::panes::kimi`: the payload parser, the band
// and its two thresholds, and the gap kinds. Read that module for the
// reasoning behind any of them (there is no per-device setup here, unlike
// `waste.ts`'s bound page URL — the credential lives in the poller's own
// Actions secret).
//
// What stayed here is what ADR-0025 leaves per-client: **the words and the
// glyph**. `formatUsd`, `kimiCollapsedHeadline`, `kimiGlyph` and
// `kimiGapReason`. Two clients disagreeing about the band would be a bug;
// two clients wording "$0.42 — critical" differently is a design choice.

/** These four constants stay literal TS, pinned against `kimi_constants_json()`
 * by `seam.test.ts` — `question.ts` builds `sources: [SOURCE]` at module
 * evaluation, before `initDecisions()` ever resolves, exactly
 * `waste.ts`'s own arrangement. */
export const SOURCE = "kimi-balance/v1";
export const SNAPSHOT_KEY = "balance";
export const STALE_AFTER_MS = 13 * 60 * 60 * 1000;
export const IMMINENT_THRESHOLD_USD = 1;
export const NEAR_THRESHOLD_USD = 5;

/** The `kimi-balance/v1` payload body — the shape is pinned by
 * `kimi.rs`'s `parse_kimi_body`; this is its wire form. */
export interface KimiBody {
  availableBalance: number;
  voucherBalance: number;
  cashBalance: number;
}

/** A body that could be read, or the reason it could not — the "gap, not
 * absence" split `waste.ts` also uses. `reason` is words a pane can render,
 * composed here from the core's gap **kind**. */
export type KimiParse = { kind: "ok"; body: KimiBody } | { kind: "gap"; reason: string };

/** Reads one snapshot row into a body, or says why it could not —
 * `kimi.rs`'s `parse_kimi_body` with this client's wording put back on. */
export function parseKimiBody(snapshot: PaneSnapshotDTO | undefined): KimiParse {
  const parsed = parseKimiBodyFromCore(snapshot);
  return parsed.kind === "ok" ? { kind: "ok", body: parsed.body } : { kind: "gap", reason: gapReason(parsed.gap) };
}

/** Re-exported from the shell for the same reason `waste.ts` re-exports it. */
export { isStaleFreshness };

/** How close `availableBalance` is to the `$0` cliff — `kimi.rs`'s
 * `kimi_band`. */
export function kimiBand(availableBalance: number): "live" | "imminent" | "near" | "dormant" {
  return kimiBandFromCore(availableBalance) as "live" | "imminent" | "near" | "dormant";
}

/** Two decimal places, a leading `$`, and the sign in front of the symbol
 * (`-$1.00`, never `$-1.00`) — the reading a negative `cash_balance`
 * actually needs. Pure rendering, so it stays here. */
export function formatUsd(amount: number): string {
  const sign = amount < 0 ? "-" : "";
  return `${sign}$${Math.abs(amount).toFixed(2)}`;
}

/** The collapsed row's whole sentence — the amount, and the decision the
 * band already made about it. */
export function kimiCollapsedHeadline(availableBalance: number): string {
  const amount = formatUsd(availableBalance);
  switch (kimiBand(availableBalance)) {
    case "live":
      return `${amount} — exhausted`;
    case "imminent":
      return `${amount} — critical`;
    case "near":
      return `${amount} — running low`;
    default:
      return `${amount} left`;
  }
}

/** One glyph naming the band — a screen reader's only way to know what the
 * dot means. */
export function kimiGlyph(availableBalance: number): PaneGlyph {
  const band = kimiBand(availableBalance);
  if (band === "live" || band === "imminent") {
    return { kind: "icon", name: "siren", label: `kimi balance ${band === "live" ? "exhausted" : "critical"}` };
  }
  return { kind: "icon", name: "database", label: "kimi balance" };
}

/** Everything an answered pane needs, read by both the answer and the
 * expanded rendering — `KimiFacts` (`kimi.rs`), `null` when this question
 * has no answer. */
export interface KimiView {
  body: KimiBody;
  stale: boolean;
  freshness: FreshnessDTO;
}

function paneInputs(inputs: QuestionInputs): PaneInputsSource {
  return { nowMs: inputs.nowMs, bindings: inputs.bindings, paneReads: inputs.paneReads };
}

function toView(facts: KimiFactsCore): KimiView {
  return {
    body: {
      availableBalance: facts.availableBalance,
      voucherBalance: facts.voucherBalance,
      cashBalance: facts.cashBalance,
    },
    stale: facts.stale,
    freshness: facts.freshness,
  };
}

/** The whole answered view, or `null` when there is nothing to answer with
 * (never polled, a payload that could not be read). */
export function kimiView(inputs: QuestionInputs): KimiView | null {
  const resolved = kimiFactsFromCore(paneInputs(inputs));
  return resolved.kind === "facts" ? toView(resolved) : null;
}

const UNRESOLVABLE = "No balance answer yet.";

function gapReason(gap: KimiGap): string {
  switch (gap.gap) {
    case "notFetched":
      return "No balance has been fetched yet.";
    case "malformed":
      return `The balance payload couldn't be read: ${gap.reason}`;
    case "unknownSchema":
      return `This device doesn't know how to read ${gap.schema} yet. Update the app.`;
    case "notJson":
      return "The balance payload isn't JSON.";
    case "notAnObject":
      return "The balance payload isn't an object.";
    case "badNumbers":
      return "The balance payload's numbers can't be read.";
    default:
      return UNRESOLVABLE;
  }
}

/** Why this pane has no answer, in words — read only when [`kimiView`]
 * returned `null`. */
export function kimiGapReason(inputs: QuestionInputs): string {
  const resolved = kimiFactsFromCore(paneInputs(inputs));
  return resolved.kind === "gap" ? gapReason(resolved.gap) : UNRESOLVABLE;
}

/** This question's answer for the shell (#313 over #245/ADR-0017). */
export function kimiAnswer(inputs: QuestionInputs): PaneAnswer {
  const source = paneInputs(inputs);
  const answer = kimiAnswerFromCore(source);
  if (answer.answerState !== "answered") {
    return {
      ...answer,
      collapsedHeadline: "No answer yet",
      icon: [{ kind: "icon", name: "cloud-fog", label: "no answer yet" }],
    };
  }

  const view = kimiView(inputs);
  if (view === null) {
    // Unreachable: the core answers `answered` only when it produced facts.
    return { ...answer, collapsedHeadline: "No answer yet" };
  }
  return {
    ...answer,
    collapsedHeadline: kimiCollapsedHeadline(view.body.availableBalance),
    icon: [kimiGlyph(view.body.availableBalance)],
  };
}
