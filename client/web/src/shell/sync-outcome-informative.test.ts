import { describe, expect, it } from "vitest";
import { isInformativeSyncOutcomeFromCore } from "../decisions/seam";
import type { TaskRunOutcomeKind } from "../store/protocol";
import { isInformativeSyncOutcome } from "./sync-outcome-informative";

// Pinned against `hummingbird_core::decisions::settings::
// is_informative_sync_outcome`'s real answer — see this file's own module
// header for why the predicate itself has to stay a literal TS copy
// rather than a live seam call.
//
// `EVERY_KIND` is derived from an exhaustive `Record`, not a hand-listed
// array — `sync-status.ts`'s pre-#535 `OUTCOME_CLASS` was exactly this
// shape for a named historical reason (round-2 review of PR #185:
// `"skipped"`/`"busy"` were in neither of the old array-shaped
// classifier's two lists and fell through to the success branch,
// re-greening the badge to "Synced — as of just now" every 60s for a
// whole outage). A `Record<TaskRunOutcomeKind, true>` is a compile error
// until every member of the union has a key, so a new outcome kind added
// to `TaskRunOutcomeKind` fails this file's build rather than silently
// skipping the pin.
const EVERY_KIND_RECORD: Record<TaskRunOutcomeKind, true> = {
  no_credential: true,
  held: true,
  skipped: true,
  blocked: true,
  credential_needed: true,
  persist_failed: true,
  pull_failed: true,
  completed: true,
  busy: true,
};
const EVERY_KIND = Object.keys(EVERY_KIND_RECORD) as TaskRunOutcomeKind[];

describe("isInformativeSyncOutcome", () => {
  it.each(EVERY_KIND)("agrees with the core's own answer for %s", (kind) => {
    expect(isInformativeSyncOutcome(kind)).toBe(isInformativeSyncOutcomeFromCore(kind));
  });
});
