import type { FreshnessDTO, PaneReadDTO, PaneSnapshotDTO } from "../../store/protocol";
import type { PaneAnswer, PaneGlyph, QuestionInputs } from "../questions/contract";
import { isStaleFreshness } from "../questions/freshness";

// **The Kimi balance question** (#313, ADR-0017 decision 5) — "what's left
// of my Moonshot balance, and how close is it to the cliff?"
//
// There is no per-device setup here, unlike `waste.ts`'s bound page URL: the
// credential and the host both live in the poller's own Actions
// secrets/variables (the brief's "Operator gate", never a `settings` row a
// device configures), so this question has no `unbound` state at all — it
// is either never-polled (`bound-but-unacquired`, the same sentinel-subject
// reading `contract.ts` documents for #311's placeholder era) or answered.
//
// **Band from the balance itself, never from `cash_balance` alone.**
// `cash_balance` can go negative while `available_balance` — the number
// Moonshot actually enforces `exceeded_current_quota_error` against — stays
// positive, so banding on the sub-fields would read a healthy account as
// exhausted or vice versa. `available_balance` is read as a countdown to the
// `$0` cliff: the thresholds below are this pane's own (ADR-0015: the
// driver is the cost of a wrong answer, not a multiple of anything), and
// they exist to answer "how much room is left", not to fabricate a days-of-
// runway estimate — ADR-0017 decision 5 explicitly rejects inferring a burn
// rate from noisy deltas, and this pane never tries: it has exactly one
// gauge, replaced wholesale each poll, and no history to derive a rate from.

/** Both the snapshot's source — ADR-0017 decision 5 enrols this in
 * `server/domain/src/sources.rs` as `Writes::Snapshots`, so no alert ever
 * carries it and there is no `key_recipe` to agree on. Nothing on this side
 * checks that registry (ADR-0015 forbids checking a snapshot's `schema`
 * against it), so this constant is this file's own, and the two agree by
 * review, not by import — `waste.ts`'s own note, verbatim. */
export const SOURCE = "kimi-balance/v1";

/** The one `context_snapshots.key` this question reads, and its subject
 * key — the sentinel subject this question always returns, polled or not. */
export const SNAPSHOT_KEY = "balance";

/** ~13h against a 6h cadence: two missed polls means the countdown has gone
 * unobserved for half a day. The threshold is set by the cost of a wrong
 * answer, not as a bare `2 ×` of the interval — the same reasoning
 * `waste.ts` gives 26h against a daily cadence rather than 48h. */
export const STALE_AFTER_MS = 13 * 60 * 60 * 1000;

/** Below this many dollars, the very next call could plausibly trip
 * `exceeded_current_quota_error` — the cliff is imminent. */
export const IMMINENT_THRESHOLD_USD = 1;

/** Below this many dollars there is still a real cushion, but not much of
 * one — worth naming as "running low" rather than silence. */
export const NEAR_THRESHOLD_USD = 5;

/** The `kimi-balance/v1` payload body — this pane's parser is what pins the
 * shape, since the body inside the envelope is deliberately unfrozen and
 * opaque to the server. Field names mirror the wire exactly:
 * `available_balance`/`voucher_balance`/`cash_balance`. */
export interface KimiBody {
  availableBalance: number;
  voucherBalance: number;
  cashBalance: number;
}

/** A body that could be read, or the reason it could not — the "gap, not
 * absence" split `waste.ts` also uses. */
export type KimiParse = { kind: "ok"; body: KimiBody } | { kind: "gap"; reason: string };

function isFiniteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

/** Reads one snapshot row into a body, or says why it could not. Every arm
 * names what was wrong, on `parseWasteBody`'s own reasoning — including the
 * "this device is behind" reading for an unrecognised `schema`. */
export function parseKimiBody(snapshot: PaneSnapshotDTO | undefined): KimiParse {
  if (snapshot === undefined) {
    return { kind: "gap", reason: "No balance has been fetched yet." };
  }
  if (snapshot.envelope.kind === "malformed") {
    return { kind: "gap", reason: `The balance payload couldn't be read: ${snapshot.envelope.reason}` };
  }
  if (snapshot.envelope.schema !== SOURCE) {
    return {
      kind: "gap",
      reason: `This device doesn't know how to read ${snapshot.envelope.schema} yet. Update the app.`,
    };
  }

  let parsed: unknown;
  try {
    parsed = JSON.parse(snapshot.envelope.body);
  } catch {
    return { kind: "gap", reason: "The balance payload isn't JSON." };
  }
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    return { kind: "gap", reason: "The balance payload isn't an object." };
  }

  const body = parsed as {
    available_balance?: unknown;
    voucher_balance?: unknown;
    cash_balance?: unknown;
  };
  if (
    !isFiniteNumber(body.available_balance) ||
    !isFiniteNumber(body.voucher_balance) ||
    !isFiniteNumber(body.cash_balance)
  ) {
    return { kind: "gap", reason: "The balance payload's numbers can't be read." };
  }

  return {
    kind: "ok",
    body: {
      availableBalance: body.available_balance,
      voucherBalance: body.voucher_balance,
      cashBalance: body.cash_balance,
    },
  };
}

/** Re-exported from the shell for the same reason `waste.ts` re-exports it:
 * #313's own threshold stays here, per ADR-0015, but the `"unknown"` reading
 * must never be copied into a second comparison a pane could get wrong. */
export { isStaleFreshness };

/** How close `availableBalance` is to the `$0` cliff — never derived from
 * `cash_balance`, which can be negative while the account is nowhere near
 * exhausted (a voucher balance still covers it). */
export function kimiBand(availableBalance: number): "live" | "imminent" | "near" | "dormant" {
  if (availableBalance <= 0) {
    return "live";
  }
  if (availableBalance < IMMINENT_THRESHOLD_USD) {
    return "imminent";
  }
  if (availableBalance < NEAR_THRESHOLD_USD) {
    return "near";
  }
  return "dormant";
}

/** Two decimal places, a leading `$`, and the sign in front of the symbol
 * (`-$1.00`, never `$-1.00`) — the reading a negative `cash_balance`
 * actually needs. */
export function formatUsd(amount: number): string {
  const sign = amount < 0 ? "-" : "";
  return `${sign}$${Math.abs(amount).toFixed(2)}`;
}

/** The collapsed row's whole sentence — the amount, and the decision the
 * band already made about it, per `contract.ts`'s "carries the decision,
 * not the datum". */
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
 * dot means, per `PaneGlyph`'s own contract. */
export function kimiGlyph(availableBalance: number): PaneGlyph {
  const band = kimiBand(availableBalance);
  if (band === "live" || band === "imminent") {
    return { kind: "icon", name: "siren", label: `kimi balance ${band === "live" ? "exhausted" : "critical"}` };
  }
  return { kind: "icon", name: "database", label: "kimi balance" };
}

/** Everything an answered pane needs, computed once and read by both the
 * answer and the expanded rendering. */
export interface KimiView {
  body: KimiBody;
  snapshot: PaneSnapshotDTO;
  stale: boolean;
  freshness: FreshnessDTO;
}

function snapshotFor(read: PaneReadDTO | undefined): PaneSnapshotDTO | undefined {
  return read?.snapshots.find((snapshot) => snapshot.key === SNAPSHOT_KEY);
}

type KimiResolve = { kind: "view"; view: KimiView } | { kind: "gap"; reason: string };

function resolveKimi(inputs: QuestionInputs): KimiResolve {
  const snapshot = snapshotFor(inputs.paneReads[SOURCE]);
  const parsed = parseKimiBody(snapshot);
  if (parsed.kind !== "ok" || snapshot === undefined) {
    return { kind: "gap", reason: parsed.kind === "gap" ? parsed.reason : "No balance answer yet." };
  }
  return {
    kind: "view",
    view: {
      body: parsed.body,
      snapshot,
      stale: isStaleFreshness(snapshot.freshness, STALE_AFTER_MS),
      freshness: snapshot.freshness,
    },
  };
}

/** The whole answered view, or `null` when there is nothing to answer with
 * (never polled, a payload that could not be read). */
export function kimiView(inputs: QuestionInputs): KimiView | null {
  const resolved = resolveKimi(inputs);
  return resolved.kind === "view" ? resolved.view : null;
}

/** Why this pane has no answer, in words — read only when [`kimiView`]
 * returned `null`. */
export function kimiGapReason(inputs: QuestionInputs): string {
  const resolved = resolveKimi(inputs);
  return resolved.kind === "gap" ? resolved.reason : "No balance answer yet.";
}

/** This question's answer for the shell (#313 over #245/ADR-0017).
 *
 * `withinBand` is `null` in every case: this pane has exactly one gauge and
 * no history to derive a time-of-cliff estimate from (ADR-0017 decision 5
 * rejects inferring a burn rate from noisy deltas), and there is only ever
 * one subject for this question, so there is nothing to tie-break against.
 * Fabricating an instant here would be a second place this pane invents
 * numbers it does not have. */
export function kimiAnswer(inputs: QuestionInputs): PaneAnswer {
  const view = kimiView(inputs);
  if (view === null) {
    return {
      answerState: "bound-but-unacquired",
      band: "dormant",
      withinBand: null,
      collapsedHeadline: "No answer yet",
      icon: [{ kind: "icon", name: "cloud-fog", label: "no answer yet" }],
    };
  }

  return {
    answerState: "answered",
    band: kimiBand(view.body.availableBalance),
    withinBand: null,
    collapsedHeadline: kimiCollapsedHeadline(view.body.availableBalance),
    icon: [kimiGlyph(view.body.availableBalance)],
  };
}
