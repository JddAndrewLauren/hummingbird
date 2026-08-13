import type { FreshnessDTO, PaneReadDTO, PaneSnapshotDTO } from "../../store/protocol";
import type { Band, PaneAnswer, PaneGlyph, QuestionInputs } from "../questions/contract";
import { isStaleFreshness } from "../questions/freshness";

// **The uptime question** (#315, ADR-0017 decisions 2/3/4/6) — "is the
// authority, the web origin and the runner answering HTTP right now?"
//
// One source, many panes, spanning two platforms: `server/uptime-probe`
// writes one `context_snapshots` row per service declared in its committed
// `services.json` — the authority and the web origin (both Cloudflare), and
// the runner (Fly) — keyed by the service's own `id`, all under this one
// source string (ADR-0017 decision 2's deliberate refinement: splitting by
// platform would need two `ingest` tokens for one credential-free binary
// that makes no such distinction itself). Subjects come from *which keys
// this source has ever written a row for*, exactly `github.ts`'s own shape
// — never a per-device binding, since nothing here is something a device
// configures.
//
// **The sweeper has no row here at all.** ADR-0017 decision 4 excludes it
// from this axis entirely: it never opens a listener, so no truthful
// `url`/`method`/`expect_status` triple exists for it and neither
// `expected` value would describe it honestly. Its own liveness signal is
// healthchecks.io, outside this pane.
//
// **Divergence from the manifest's stated intent is the only thing that
// lifts a band** (ADR-0017 decision 4) — an `expected: "off"` service that
// is correctly unreachable reads as quiet agreement (`dormant`), never as a
// status word; an `expected: "on"` service that cannot be reached, or an
// `expected: "off"` service that unexpectedly answers, are the two faults
// this pane exists to surface. The band is computed here, at read time,
// from the raw observation the poller wrote — never from a precomputed
// verdict (`body.rs`'s own header).
//
// **A probe that cannot tell the truth must never render as healthy.** An
// unreachable host, a DNS failure, a timeout, a malformed payload, and the
// probe workflow itself going stale each get their own distinct, visible
// reading — never a silent fall-through to "healthy" (the class of bug that
// sank #314 twice).

/** Both the snapshot's source and the envelope's `schema` — ADR-0017
 * decision 2 enrols this in `server/domain/src/sources.rs` as
 * `Writes::Snapshots`, so no alert ever carries it and there is no
 * `key_recipe` to agree on. Nothing on this side checks that registry
 * (ADR-0015 forbids resolving a snapshot's `schema` against it), so this
 * constant is this file's own, and the two agree by review, not by import —
 * `waste.ts`'s own note, verbatim. */
export const SOURCE = "uptime/v1";

/** The one sentinel subject the never-polled state emits — never a real
 * service id (`authority`/`web`/`runner`), so it can never collide with a
 * genuine key. */
export const NEVER_POLLED_SUBJECT = "pending";

/** ~3h against the probe workflow's hourly cadence (ADR-0017 decision 6):
 * three missed runs is worse here than for any other infra pane, because
 * this lane is the one that would otherwise notice every *other* service
 * going dark — the same "the pane that watches for outages must not itself
 * go quiet unnoticed" reasoning `github.ts` gives its own ~30h line against
 * a daily poller, scaled to this poller's much tighter hourly cadence. */
export const STALE_AFTER_MS = 3 * 60 * 60 * 1000;

/** The `uptime/v1` payload body — this pane's parser is what pins the
 * shape, since the body inside the envelope is deliberately unfrozen and
 * opaque to the server. Field names mirror the wire exactly. */
export interface ProbeBody {
  expected: "on" | "off";
  expectStatus: number;
  observedStatus: number | null;
  error: string | null;
}

/** A body that could be read, or the reason it could not — the "gap, not
 * absence" split every pane in this region uses. */
export type ProbeParse = { kind: "ok"; body: ProbeBody } | { kind: "gap"; reason: string };

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
 * "this device is behind" reading for an unrecognised `schema`, and a
 * malformed payload is its own distinct gap reason — a probe that could not
 * be read must never fall through to a healthy-looking reading. */
export function parseUptimeBody(snapshot: PaneSnapshotDTO | undefined): ProbeParse {
  if (snapshot === undefined) {
    return { kind: "gap", reason: "No answer has been fetched yet." };
  }
  if (snapshot.envelope.kind === "malformed") {
    return { kind: "gap", reason: `The probe payload couldn't be read: ${snapshot.envelope.reason}` };
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
    return { kind: "gap", reason: "The probe payload isn't JSON." };
  }
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    return { kind: "gap", reason: "The probe payload isn't an object." };
  }

  const body = parsed as {
    expected?: unknown;
    expect_status?: unknown;
    observed_status?: unknown;
    error?: unknown;
  };
  if (
    (body.expected !== "on" && body.expected !== "off") ||
    !isFiniteNumber(body.expect_status) ||
    !isNullableFiniteNumber(body.observed_status) ||
    !isNullableString(body.error)
  ) {
    return { kind: "gap", reason: "The probe payload's fields can't be read." };
  }
  // Mutually exclusive by construction on the wire (`ProbeBody::from_outcome`)
  // — a payload claiming both or neither is malformed, not a third reading
  // this pane should try to interpret.
  if ((body.observed_status === null) === (body.error === null)) {
    return { kind: "gap", reason: "The probe payload's observation can't be read." };
  }

  return {
    kind: "ok",
    body: {
      expected: body.expected,
      expectStatus: body.expect_status,
      observedStatus: body.observed_status,
      error: body.error,
    },
  };
}

/** Re-exported from the shell for the same reason `waste.ts` and `kimi.ts`
 * both do: the `"unknown"` reading must never be copied into a second
 * comparison a pane could get wrong. */
export { isStaleFreshness };

/** This service's band, at read time, from the raw observation alone —
 * never from a stored judgement (`body.rs`'s own header).
 *
 * **Divergence from the manifest's stated intent is the only thing that
 * lifts a band** (ADR-0017 decision 4): an `expected: "off"` service
 * correctly unreachable is quiet agreement, `dormant`; an `expected: "on"`
 * service that cannot be reached at all is the most severe reading,
 * `live`, since the door itself is shut; a reachable-but-wrong-status
 * service is a lesser fault (the door is open, but not answering as
 * declared), `near`; and an `expected: "off"` service that unexpectedly
 * answers — "something is running that should not be" — is treated as
 * severely as a fully-down `expected: "on"` service, `live`, since both
 * are the manifest's stated intent being wrong right now. */
export function uptimeBand(body: ProbeBody): Band {
  if (body.expected === "off") {
    return body.error !== null ? "dormant" : "live";
  }
  if (body.error !== null) {
    return "live";
  }
  if (body.observedStatus !== body.expectStatus) {
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

/** The collapsed row's whole sentence, naming the service (`github.ts`'s
 * own reasoning: with several services the shell draws this question's
 * label once and only this line can say which pane is which).
 *
 * Every distinct failure shape gets its own distinct wording — an
 * unreachable host's own error text, a wrong status code, or an
 * unexpectedly-answering service that should be off — never a shared
 * "unhealthy" that would erase the difference between them. */
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

/** One glyph naming the band — a screen reader's only way to know what the
 * dot means, per `PaneGlyph`'s own contract. */
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

/** Everything one answered pane needs, computed once and read by both the
 * answer and the expanded rendering. */
export interface ProbeView {
  serviceId: string;
  body: ProbeBody;
  snapshot: PaneSnapshotDTO;
  stale: boolean;
  freshness: FreshnessDTO;
}

/** This question's subjects: one per service key this source has ever
 * written a row for, sorted for a stable render order independent of the
 * server's own response order — or the one sentinel subject when nothing
 * has been read yet (ADR-0017/#311's "a platform with no rows yet renders
 * as a gap, never as nothing"). */
export function uptimeSubjects(inputs: QuestionInputs): string[] {
  const read = inputs.paneReads[SOURCE];
  if (read === undefined || read.snapshots.length === 0) {
    return [NEVER_POLLED_SUBJECT];
  }
  return read.snapshots.map((snapshot) => snapshot.key).slice().sort();
}

type ProbeResolve = { kind: "view"; view: ProbeView } | { kind: "gap"; reason: string };

function resolveProbe(serviceId: string, inputs: QuestionInputs): ProbeResolve {
  const read: PaneReadDTO | undefined = inputs.paneReads[SOURCE];
  const snapshot = read?.snapshots.find((candidate) => candidate.key === serviceId);
  const parsed = parseUptimeBody(snapshot);
  if (parsed.kind !== "ok" || snapshot === undefined) {
    return { kind: "gap", reason: parsed.kind === "gap" ? parsed.reason : "No answer yet." };
  }
  return {
    kind: "view",
    view: {
      serviceId,
      body: parsed.body,
      snapshot,
      stale: isStaleFreshness(snapshot.freshness, STALE_AFTER_MS),
      freshness: snapshot.freshness,
    },
  };
}

/** The whole answered view, or `null` when there is nothing to answer with
 * (never polled, a payload that could not be read). */
export function uptimeView(serviceId: string, inputs: QuestionInputs): ProbeView | null {
  const resolved = resolveProbe(serviceId, inputs);
  return resolved.kind === "view" ? resolved.view : null;
}

/** Why this pane has no answer, in words — read only when [`uptimeView`]
 * returned `null`. */
export function uptimeGapReason(serviceId: string, inputs: QuestionInputs): string {
  const resolved = resolveProbe(serviceId, inputs);
  return resolved.kind === "gap" ? resolved.reason : "No answer yet.";
}

/** This question's answer for the shell (#315 over ADR-0017).
 *
 * `withinBand` is `null` throughout: there is no "next relevant moment" a
 * reachability probe names — the pane's whole story is the present instant,
 * on `kimi.ts`'s own reasoning for its single gauge.
 *
 * **The probe workflow itself going quiet must not read as continued
 * agreement.** Read from the payload alone, a `dormant` band is only as
 * trustworthy as the poller that wrote it — once the hourly workflow itself
 * stops running, every row it ever wrote keeps reporting whatever it last
 * saw, forever. A stale `dormant` reading is therefore escalated to
 * `imminent` here, before it ever reaches the collapsed row — both lifting
 * the band (so the pane opens instead of collapsing) and naming the
 * staleness in the headline, on `github.ts`'s own escalation. A non-`dormant`
 * reading is left alone: it is already visible, and going stale on top of
 * an already-reported divergence does not need a second escalation. */
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

  const view = uptimeView(subjectKey, inputs);
  if (view === null) {
    return {
      answerState: "bound-but-unacquired",
      band: "dormant",
      withinBand: null,
      collapsedHeadline: "No answer yet",
      icon: [{ kind: "icon", name: "cloud-fog", label: "no answer yet" }],
    };
  }

  const band = uptimeBand(view.body);
  if (view.stale && band === "dormant") {
    const heardAgo = view.freshness.kind === "age" ? ageWords(view.freshness.ageMs) : "an unknown time ago";
    return {
      answerState: "answered",
      band: "imminent",
      withinBand: null,
      collapsedHeadline: `${view.serviceId} · answer may be stale, last heard ${heardAgo}`,
      icon: [{ kind: "icon", name: "cloud-fog", label: `${view.serviceId} answer may be stale` }],
    };
  }

  return {
    answerState: "answered",
    band,
    withinBand: null,
    collapsedHeadline: uptimeCollapsedHeadline(view.serviceId, view.body),
    icon: [uptimeGlyph(view.serviceId, view.body)],
  };
}
