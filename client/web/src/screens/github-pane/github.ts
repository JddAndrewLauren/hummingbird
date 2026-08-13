import type { FreshnessDTO, PaneReadDTO, PaneSnapshotDTO } from "../../store/protocol";
import type { PaneAnswer, PaneGlyph, QuestionInputs } from "../questions/contract";
import { isStaleFreshness } from "../questions/freshness";

// **The GitHub workflow-health question** (#314, ADR-0017 decision 2) —
// "are hummingbird's own scheduled workflows still running at all?"
//
// One source, many panes: `server/github-status` writes one
// `context_snapshots` row per scheduled workflow it finds in this repo's own
// `.github/workflows/`, keyed by the workflow's file name, all under this
// one source string. This question's `subjects()` is therefore the first on
// the Status surface whose subject list comes from *which keys this source
// has ever written a row for* rather than from a per-device binding
// (`race.ts`'s `race-series` list) or a fixed sentinel (`kimi.ts`'s single
// `"balance"` key) — a workflow shows up here the moment the poller has ever
// written a row for it, and disappears from the pane only if it is deleted
// from the repo (its snapshot row simply stops being written to, and ages
// out of relevance the same way any dormant row does).
//
// There is no per-device setup here, on `kimi.ts`'s own reasoning: the
// credential (`HB_INGEST_TOKEN`) lives in the poller's own Actions secret,
// never a `settings` row a device configures. So this question has no
// `unbound` state — it is either never-polled at all (`bound-but-unacquired`,
// one sentinel subject) or answered, one pane per workflow key this source
// has ever written.
//
// **The band is computed here, at read time, from the raw verdict the
// poller wrote — never from a precomputed "stalled" flag** (`body.rs`'s own
// header: a stored judgement is a fact that can disagree with itself between
// the moment the poller ran and the moment a reader looks). "A stopped cron
// outranks a single failure" (the brief's own phrasing) is exactly the
// ordering `githubBand` encodes below.

/** Both the snapshot's source and the envelope's `schema` — ADR-0017
 * decision 2 enrols this in `server/domain/src/sources.rs` as
 * `Writes::Snapshots`, so no alert ever carries it and there is no
 * `key_recipe` to agree on. Nothing on this side checks that registry
 * (ADR-0015 forbids resolving a snapshot's `schema` against it), so this
 * constant is this file's own and the two agree by review, not by import —
 * `waste.ts`'s own note, verbatim. */
export const SOURCE = "github-hummingbird/v1";

/** The one sentinel subject the never-polled state emits — never a real
 * workflow file name (every real one ends `.yml`/`.yaml`), so it can never
 * collide with a genuine key. */
export const NEVER_POLLED_SUBJECT = "pending";

/** ~30h against `github-status.yml`'s own daily cadence: the cost of a
 * *late* answer here is worse than `waste.ts`'s 26h, because this pane's
 * whole purpose is catching a dead cron quickly — waiting a full second day
 * (`2 × 24h`) to notice the poller that watches for dead crons has itself
 * gone quiet would defeat the point (the brief's own "stale threshold set
 * from the cost of a wrong answer, not as a multiple of the daily cadence"). */
export const STALE_AFTER_MS = 30 * 60 * 60 * 1000;

/** A scheduled run is judged overdue once its last success is this many
 * multiples of its own declared cadence old. `3×` rather than `2×`
 * (`race.ts`'s plain multiple): GitHub Actions' own cron carries more slack
 * than a self-hosted clock (documented delivery jitter, and this repo's own
 * 60-day auto-disable is the actual failure mode this pane exists to catch,
 * not a single missed tick), so `2×` would flag routine jitter as an
 * incident. */
export const OVERDUE_MULTIPLIER = 3;

/** The `github-hummingbird/v1` payload body — this pane's parser is what
 * pins the shape, since the body inside the envelope is deliberately
 * unfrozen and opaque to the server. Field names mirror the wire exactly. */
export interface WorkflowBody {
  displayName: string;
  declaredCadenceMs: number | null;
  lastRunConclusion: string | null;
  lastRunEvent: string | null;
  lastRunAtMs: number | null;
  lastScheduledSuccessAtMs: number | null;
}

/** A body that could be read, or the reason it could not — the "gap, not
 * absence" split every pane in this region uses. */
export type WorkflowParse = { kind: "ok"; body: WorkflowBody } | { kind: "gap"; reason: string };

function isFiniteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function isNullableFiniteNumber(value: unknown): value is number | null {
  return value === null || isFiniteNumber(value);
}

function isNullableString(value: unknown): value is string | null {
  return value === null || typeof value === "string";
}

/** Reads one snapshot row into a body, or says why it could not. Every arm
 * names what was wrong, on `parseWasteBody`'s own reasoning — including the
 * "this device is behind" reading for an unrecognised `schema`. */
export function parseWorkflowBody(snapshot: PaneSnapshotDTO | undefined): WorkflowParse {
  if (snapshot === undefined) {
    return { kind: "gap", reason: "No answer has been fetched yet." };
  }
  if (snapshot.envelope.kind === "malformed") {
    return { kind: "gap", reason: `The workflow payload couldn't be read: ${snapshot.envelope.reason}` };
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
    return { kind: "gap", reason: "The workflow payload isn't JSON." };
  }
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    return { kind: "gap", reason: "The workflow payload isn't an object." };
  }

  const body = parsed as {
    display_name?: unknown;
    declared_cadence_ms?: unknown;
    last_run_conclusion?: unknown;
    last_run_event?: unknown;
    last_run_at_ms?: unknown;
    last_scheduled_success_at_ms?: unknown;
  };
  if (
    typeof body.display_name !== "string" ||
    !isNullableFiniteNumber(body.declared_cadence_ms) ||
    !isNullableString(body.last_run_conclusion) ||
    !isNullableString(body.last_run_event) ||
    !isNullableFiniteNumber(body.last_run_at_ms) ||
    !isNullableFiniteNumber(body.last_scheduled_success_at_ms)
  ) {
    return { kind: "gap", reason: "The workflow payload's fields can't be read." };
  }

  return {
    kind: "ok",
    body: {
      displayName: body.display_name,
      declaredCadenceMs: body.declared_cadence_ms,
      lastRunConclusion: body.last_run_conclusion,
      lastRunEvent: body.last_run_event,
      lastRunAtMs: body.last_run_at_ms,
      lastScheduledSuccessAtMs: body.last_scheduled_success_at_ms,
    },
  };
}

/** Re-exported from the shell for the same reason `waste.ts` and `kimi.ts`
 * both do: the `"unknown"` reading must never be copied into a second
 * comparison a pane could get wrong. */
export { isStaleFreshness };

/** Every conclusion GitHub's own API reports that is NOT a success —
 * anything else (including a still-running run's `null`) is not read as a
 * failure at all, since "no conclusion yet" is not "failed". */
const FAILURE_CONCLUSIONS = new Set([
  "failure",
  "cancelled",
  "timed_out",
  "action_required",
  "startup_failure",
]);

/** This workflow's band, at read time, from the raw verdict alone — never
 * from a stored judgement (`body.rs`'s own header).
 *
 * Ranked exactly as the brief orders them: a cron that has **never run at
 * all** (the auto-disable tell) outranks one that is merely **overdue**
 * against its own declared cadence, which outranks a **single failed run**
 * that the cron is otherwise still meeting on schedule, which outranks a
 * healthy workflow. */
export function githubBand(body: WorkflowBody, nowMs: number): "live" | "imminent" | "near" | "dormant" {
  if (body.lastRunAtMs === null) {
    return "live";
  }
  if (body.lastScheduledSuccessAtMs === null) {
    // Has run, but never a *scheduled* success in this window — a manual
    // run must never mask this (#314's own named scenario).
    return "imminent";
  }
  if (body.declaredCadenceMs !== null) {
    const overdueAfterMs = body.declaredCadenceMs * OVERDUE_MULTIPLIER;
    if (nowMs - body.lastScheduledSuccessAtMs > overdueAfterMs) {
      return "imminent";
    }
  }
  if (body.lastRunConclusion !== null && FAILURE_CONCLUSIONS.has(body.lastRunConclusion)) {
    return "near";
  }
  return "dormant";
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

/** The collapsed row's whole sentence, naming the workflow (`race.ts`'s own
 * reasoning: with several workflows the shell draws this question's label
 * once and only this line can say which pane is which). */
export function githubCollapsedHeadline(body: WorkflowBody, nowMs: number): string {
  const band = githubBand(body, nowMs);
  switch (band) {
    case "live":
      return `${body.displayName} · never run`;
    case "imminent":
      return body.lastScheduledSuccessAtMs === null
        ? `${body.displayName} · no scheduled success`
        : `${body.displayName} · stalled, last ok ${ageWords(nowMs - body.lastScheduledSuccessAtMs)}`;
    case "near":
      return `${body.displayName} · last run failed`;
    default:
      return `${body.displayName} · healthy`;
  }
}

/** One glyph naming the band — a screen reader's only way to know what the
 * dot means, per `PaneGlyph`'s own contract. */
export function githubGlyph(body: WorkflowBody, nowMs: number): PaneGlyph {
  const band = githubBand(body, nowMs);
  if (band === "live" || band === "imminent") {
    return { kind: "icon", name: "siren", label: `${body.displayName} ${band === "live" ? "never run" : "stalled"}` };
  }
  if (band === "near") {
    return { kind: "icon", name: "bell", label: `${body.displayName} last run failed` };
  }
  return { kind: "icon", name: "circle-check", label: `${body.displayName} healthy` };
}

/** Everything one answered pane needs, computed once and read by both the
 * answer and the expanded rendering. */
export interface WorkflowView {
  fileName: string;
  body: WorkflowBody;
  snapshot: PaneSnapshotDTO;
  stale: boolean;
  freshness: FreshnessDTO;
}

/** This question's subjects: one per workflow key this source has ever
 * written a row for, sorted for a stable render order independent of the
 * server's own response order (`context_snapshots` carries no ordering
 * guarantee) — or the one sentinel subject when nothing has been read yet
 * or nothing has ever been polled (ADR-0017/#311's "a platform with no rows
 * yet renders as a gap, never as nothing"). */
export function githubSubjects(inputs: QuestionInputs): string[] {
  const read = inputs.paneReads[SOURCE];
  if (read === undefined || read.snapshots.length === 0) {
    return [NEVER_POLLED_SUBJECT];
  }
  return read.snapshots.map((snapshot) => snapshot.key).slice().sort();
}

type WorkflowResolve = { kind: "view"; view: WorkflowView } | { kind: "gap"; reason: string };

function resolveWorkflow(fileName: string, inputs: QuestionInputs): WorkflowResolve {
  const read: PaneReadDTO | undefined = inputs.paneReads[SOURCE];
  const snapshot = read?.snapshots.find((candidate) => candidate.key === fileName);
  const parsed = parseWorkflowBody(snapshot);
  if (parsed.kind !== "ok" || snapshot === undefined) {
    return { kind: "gap", reason: parsed.kind === "gap" ? parsed.reason : "No answer yet." };
  }
  return {
    kind: "view",
    view: {
      fileName,
      body: parsed.body,
      snapshot,
      stale: isStaleFreshness(snapshot.freshness, STALE_AFTER_MS),
      freshness: snapshot.freshness,
    },
  };
}

/** The whole answered view, or `null` when there is nothing to answer with
 * (never polled, a payload that could not be read). */
export function githubView(fileName: string, inputs: QuestionInputs): WorkflowView | null {
  const resolved = resolveWorkflow(fileName, inputs);
  return resolved.kind === "view" ? resolved.view : null;
}

/** Why this pane has no answer, in words — read only when [`githubView`]
 * returned `null`. */
export function githubGapReason(fileName: string, inputs: QuestionInputs): string {
  const resolved = resolveWorkflow(fileName, inputs);
  return resolved.kind === "gap" ? resolved.reason : "No answer yet.";
}

/** This question's answer for the shell (#314 over #245/ADR-0017).
 *
 * `withinBand` is `null` throughout: there is no "next relevant moment" a
 * workflow's own run history names — the pane's whole story is about the
 * past (when did it last succeed), never a future instant to sort by, on
 * `kimi.ts`'s own reasoning for its single gauge. */
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

  const view = githubView(subjectKey, inputs);
  if (view === null) {
    // A workflow key nothing could be read for — a malformed payload, or a
    // schema this build doesn't recognise. Named rather than dropped, on
    // `race.ts`'s own reasoning for a followed series with no snapshot.
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
    band: githubBand(view.body, inputs.nowMs),
    withinBand: null,
    collapsedHeadline: githubCollapsedHeadline(view.body, inputs.nowMs),
    icon: [githubGlyph(view.body, inputs.nowMs)],
  };
}
