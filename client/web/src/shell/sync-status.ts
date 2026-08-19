// S9's sync-status readout: last sweep, queue depth, offline, held — as a
// computed value, honesty-over-reassurance voice (design README: state what
// is true and stop). Mirrors `status-label.ts`'s own shape for the core's
// status line, one level down: this is the *sync cycle's* status, not the
// core/wasm load's.
//
// #535 (M4, ADR-0025): every function below except `isInformativeSyncOutcome`
// is a thin adapter over `hummingbird_core::decisions::settings`, reached
// through `decisions/seam.ts` — the sink Android's Settings screen needed
// so it did not carry its own copy of what "stale"/"held"/"synced" mean.
// This file's own exported shapes (`SyncStatusInput`) are unchanged from
// before the sink: every caller in this app keeps reading `TaskSyncOutcome`
// objects, and the mapping down to the core's plain wire `kind` string
// happens here, at the boundary.
//
// `isInformativeSyncOutcome` is re-exported from `sync-outcome-informative
// .ts` rather than routed through the seam here — that module's own header
// says why (`worker/ports.ts` needs the identical predicate and runs
// inside `core.worker.ts`'s own script evaluation, which must never
// statically reach the main-thread seam).

import {
  deadLetterHeadingFromCore,
  relativeAgeFromCore,
  syncOutcomeClassFromCore,
  syncStatusSummaryFromCore,
  type SyncStatusInputCore,
} from "../decisions/seam";
import type { TaskRunOutcomeKind } from "../store/protocol";
import type { TaskSyncOutcome } from "../store/store";
import { isInformativeSyncOutcome } from "./sync-outcome-informative";

export { isInformativeSyncOutcome };

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
 * classes — `hummingbird_core::decisions::settings::SyncOutcomeClass`'s own
 * four. See that module's doc for what each one means; this file only
 * reads the core's answer. */
export type SyncOutcomeClass = "held" | "failed" | "not-run" | "landed";

function toCoreInput(input: SyncStatusInput): SyncStatusInputCore {
  return {
    online: input.online,
    lastSyncOutcomeKind: input.lastSyncOutcome?.kind ?? null,
    lastSyncAtMs: input.lastSyncAtMs,
    queueDepth: input.queueDepth,
    nowMs: input.nowMs,
  };
}

export function syncOutcomeClass(kind: TaskRunOutcomeKind): SyncOutcomeClass {
  return syncOutcomeClassFromCore(kind);
}

/** A short relative age in the product's own register — `just now`, `12m ago`,
 * `3h ago`, `2d ago` (the design system's "Numbers and time" rule). Exported
 * because the triage inbox's collapsed rows state a capture's age the same way,
 * and two formatters would drift into two vocabularies. */
export function relativeAge(ageMs: number): string {
  return relativeAgeFromCore(ageMs);
}

export type SyncStatusTone = "neutral" | "warn" | "danger" | "success";

export function syncStatusTone(input: SyncStatusInput): SyncStatusTone {
  return syncStatusSummaryFromCore(toCoreInput(input)).tone;
}

export function syncStatusLabel(input: SyncStatusInput): string {
  return syncStatusSummaryFromCore(toCoreInput(input)).label;
}

/** The short word `SettingsScreen.tsx` badges the sync status with. */
export function syncStatusToneWord(input: SyncStatusInput): string {
  return syncStatusSummaryFromCore(toCoreInput(input)).toneWord;
}

/** The dead-letter affordance's heading — pluralised off the real count. */
export function deadLetterHeading(count: number): string {
  return deadLetterHeadingFromCore(count);
}
