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

/** The outcomes that mean "the cycle did not run because the credential
 * needs attention" — never staled by the passage of time the way a
 * transient network failure is. */
const HELD_KINDS: readonly TaskRunOutcomeKind[] = ["held", "credential_needed", "no_credential"];

/** The outcomes that mean the cycle ran but did not land — worth calling
 * "stale" rather than "synced" even though a timestamp exists. */
const FAILED_KINDS: readonly TaskRunOutcomeKind[] = ["pull_failed", "persist_failed", "blocked"];

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
  if (input.lastSyncOutcome !== null && HELD_KINDS.includes(input.lastSyncOutcome.kind)) {
    return "warn";
  }
  if (input.lastSyncAtMs === null) {
    return "neutral";
  }
  const failed = input.lastSyncOutcome !== null && FAILED_KINDS.includes(input.lastSyncOutcome.kind);
  return failed ? "danger" : "success";
}

export function syncStatusLabel(input: SyncStatusInput): string {
  if (!input.online) {
    return `Offline${queuedSuffix(input.queueDepth)}`;
  }
  if (input.lastSyncOutcome !== null && HELD_KINDS.includes(input.lastSyncOutcome.kind)) {
    return `Held — device token needed${queuedSuffix(input.queueDepth)}`;
  }
  if (input.lastSyncAtMs === null) {
    return `Not yet synced${queuedSuffix(input.queueDepth)}`;
  }
  const age = formatAge(Math.max(0, input.nowMs - input.lastSyncAtMs));
  const failed = input.lastSyncOutcome !== null && FAILED_KINDS.includes(input.lastSyncOutcome.kind);
  const state = failed ? "Stale" : "Synced";
  return `${state} — as of ${age}${queuedSuffix(input.queueDepth)}`;
}
