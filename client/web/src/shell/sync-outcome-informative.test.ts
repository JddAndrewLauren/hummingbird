import { describe, expect, it } from "vitest";
import { isInformativeSyncOutcomeFromCore } from "../decisions/seam";
import type { TaskRunOutcomeKind } from "../store/protocol";
import { isInformativeSyncOutcome } from "./sync-outcome-informative";

// Pinned against `hummingbird_core::decisions::settings::
// is_informative_sync_outcome`'s real answer — see this file's own module
// header for why the predicate itself has to stay a literal TS copy
// rather than a live seam call.
const EVERY_KIND: TaskRunOutcomeKind[] = [
  "no_credential",
  "held",
  "skipped",
  "blocked",
  "credential_needed",
  "persist_failed",
  "pull_failed",
  "completed",
  "busy",
];

describe("isInformativeSyncOutcome", () => {
  it.each(EVERY_KIND)("agrees with the core's own answer for %s", (kind) => {
    expect(isInformativeSyncOutcome(kind)).toBe(isInformativeSyncOutcomeFromCore(kind));
  });
});
