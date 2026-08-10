// S9's sync-status readout: last sweep, queue depth, offline, held — as a
// computed value, honesty-over-reassurance voice (design README: state what
// is true and stop). Mirrors `status-label.ts`'s own shape for the core's
// status line, one level down: this is the *sync cycle's* status, not the
// core/wasm load's.

import type { TaskRunOutcomeKind } from "../store/protocol";
import type { TaskSyncOutcome } from "../store/store";

export interface SyncStatusInput {
  online: boolean;
  lastSyncOutcome: TaskSyncOutcome | null;
  /** When the last cycle happened, any trigger, any outcome — `null` before
   * the first one this session. */
  lastSyncAtMs: number | null;
  queueDepth: number | null;
  nowMs: number;
}

/** What one outcome means for the staleness readout, as four disjoint
 * classes:
 *
 * - `"held"` — the cycle did not run because the credential needs
 *   attention; never staled by the passage of time the way a transient
 *   network failure is.
 * - `"failed"` — the cycle ran but did not land: "stale" rather than
 *   "synced" even though a timestamp exists.
 * - `"not-run"` — nothing was attempted at all (`cycle.rs`'s own words for
 *   `Skipped`: the ADR-0007 backoff was not ready yet; `busy` is the same
 *   for a host wedged mid-`runSync`). These carry NO information about how
 *   stale the mirror is — see [`isInformativeSyncOutcome`].
 * - `"landed"` — the cycle completed.
 */
export type SyncOutcomeClass = "held" | "failed" | "not-run" | "landed";

/** Every kind classified explicitly, as a `Record` keyed by the union
 * rather than a set of arrays with an implicit fall-through — the same
 * compile-time-exhaustive shape (and for the same reason) as
 * `worker/request-router.ts`'s `TASK_REQUEST_TYPES`. The array version had
 * a real defect: `"skipped"` and `"busy"` were in neither list, so both
 * fell through to the success branch and a backing-off cycle during a
 * server outage re-greened the badge to "Synced — as of just now" every 60
 * seconds for the whole outage. Adding a kind to `TaskRunOutcomeKind`
 * without classifying it here is now a type error, not a silent success. */
const OUTCOME_CLASS: Record<TaskRunOutcomeKind, SyncOutcomeClass> = {
  held: "held",
  credential_needed: "held",
  no_credential: "held",
  pull_failed: "failed",
  persist_failed: "failed",
  blocked: "failed",
  skipped: "not-run",
  busy: "not-run",
  completed: "landed",
};

export function syncOutcomeClass(kind: TaskRunOutcomeKind): SyncOutcomeClass {
  return OUTCOME_CLASS[kind];
}

/** Whether an outcome says anything about how stale the mirror is. A
 * `"not-run"` outcome does not: no request was made, so the last real
 * outcome — and the timestamp that goes with it — is still the truth.
 * `store/worker-client.ts` filters on this before overwriting
 * `lastSyncOutcome`/`lastSyncAtMs`, which is what keeps a "Stale" badge
 * stale across an outage's worth of backed-off ticks. The functions below
 * classify it a second time anyway (a non-landing outcome must never read
 * as success, whatever the caller passes). */
export function isInformativeSyncOutcome(kind: TaskRunOutcomeKind): boolean {
  return syncOutcomeClass(kind) !== "not-run";
}

function classOf(outcome: { kind: TaskRunOutcomeKind } | null): SyncOutcomeClass | null {
  return outcome === null ? null : syncOutcomeClass(outcome.kind);
}

function formatAge(ageMs: number): string {
  const minutes = Math.floor(ageMs / 60_000);
  if (minutes < 1) {
    return "just now";
  }
  if (minutes < 60) {
    return `${minutes}m ago`;
  }
  const hours = Math.floor(minutes / 60);
  if (hours < 24) {
    return `${hours}h ago`;
  }
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

/** The queued-count suffix, present only when there is something queued — a
 * "0 queued" pill is decoration (`Header.tsx`'s own rule for its badge). */
function queuedSuffix(queueDepth: number | null): string {
  return queueDepth !== null && queueDepth > 0 ? ` · ${queueDepth} queued` : "";
}

/** The badge tone the settings screen renders `syncStatusLabel` in —
 * matches the design system's own status colours, kept as a pure decision
 * alongside the label rather than re-derived from parsing the string. */
export type SyncStatusTone = "neutral" | "warn" | "danger" | "success";

export function syncStatusTone(input: SyncStatusInput): SyncStatusTone {
  if (!input.online) {
    return "neutral";
  }
  const outcome = classOf(input.lastSyncOutcome);
  if (outcome === "held") {
    return "warn";
  }
  if (input.lastSyncAtMs === null) {
    return "neutral";
  }
  return outcome === "failed" || outcome === "not-run" ? "danger" : "success";
}

export function syncStatusLabel(input: SyncStatusInput): string {
  if (!input.online) {
    return `Offline${queuedSuffix(input.queueDepth)}`;
  }
  const outcome = classOf(input.lastSyncOutcome);
  if (outcome === "held") {
    return `Held — device token needed${queuedSuffix(input.queueDepth)}`;
  }
  if (input.lastSyncAtMs === null) {
    return `Not yet synced${queuedSuffix(input.queueDepth)}`;
  }
  const age = formatAge(Math.max(0, input.nowMs - input.lastSyncAtMs));
  const state = outcome === "failed" || outcome === "not-run" ? "Stale" : "Synced";
  return `${state} — as of ${age}${queuedSuffix(input.queueDepth)}`;
}

/** The short word `SettingsScreen.tsx` badges the sync status with — round-1
 * review: a 4-valued tone with a 1-valued consumer (only ever checked for
 * `"danger"`) defeats the point of computing it, so every state gets its own
 * word. Round-2 review: a fixed tone→word record made the neutral badge say
 * "not syncing" next to a label saying "Offline" — inconsistent copy,
 * because `neutral` covers two distinct states. The word is computed from
 * the SAME branches as [`syncStatusTone`] and [`syncStatusLabel`] instead,
 * so badge and label can never disagree about which state they describe. */
export function syncStatusToneWord(input: SyncStatusInput): string {
  if (!input.online) {
    return "offline";
  }
  const outcome = classOf(input.lastSyncOutcome);
  if (outcome === "held") {
    return "held";
  }
  if (input.lastSyncAtMs === null) {
    return "not synced";
  }
  return outcome === "failed" || outcome === "not-run" ? "stale" : "synced";
}

/** The dead-letter affordance's heading — pluralised off the real count
 * (round-1 review: the fixed "1 edit didn't apply" string was wrong for
 * any count other than exactly one). */
export function deadLetterHeading(count: number): string {
  return `${count} edit${count === 1 ? "" : "s"} didn't apply`;
}
